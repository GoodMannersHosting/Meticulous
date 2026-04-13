//! Reconciler for AgentPool resources.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, ListParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::{Action, Controller};
use kube::runtime::watcher::Config;
use kube::{Client, ResourceExt};
use tracing::{debug, error, info, instrument, warn};

use crate::crd::{AgentPool, AgentPoolStatus, PoolCondition};
use crate::error::{OperatorError, Result};

/// Context shared across reconciliation runs.
pub struct Context {
    /// Kubernetes client.
    pub client: Client,
    /// NATS client for queue depth metrics.
    pub nats: Option<async_nats::Client>,
}

/// Reconciler for AgentPool resources.
pub struct AgentPoolReconciler;

impl AgentPoolReconciler {
    /// Start the reconciler.
    pub async fn run(client: Client, nats: Option<async_nats::Client>) -> Result<()> {
        let pools: Api<AgentPool> = Api::all(client.clone());
        let pods: Api<Pod> = Api::all(client.clone());

        let context = Arc::new(Context { client, nats });

        info!("starting AgentPool reconciler");

        Controller::new(pools, Config::default())
            .owns(pods, Config::default())
            .run(Self::reconcile, Self::error_policy, context)
            .for_each(|res| async move {
                match res {
                    Ok(o) => debug!(action = ?o, "reconciled"),
                    Err(e) => error!(error = %e, "reconcile error"),
                }
            })
            .await;

        Ok(())
    }

    /// Reconcile a single AgentPool.
    #[instrument(skip(pool, ctx), fields(pool = %pool.name_any()))]
    async fn reconcile(
        pool: Arc<AgentPool>,
        ctx: Arc<Context>,
    ) -> std::result::Result<Action, OperatorError> {
        let name = pool.name_any();
        let namespace = pool.namespace().unwrap_or_default();

        info!(name = %name, namespace = %namespace, "reconciling AgentPool");

        let pools: Api<AgentPool> = Api::namespaced(ctx.client.clone(), &namespace);
        let pods: Api<Pod> = Api::namespaced(ctx.client.clone(), &namespace);

        // List pods owned by this pool
        let label_selector = format!("meticulous.dev/pool={}", name);
        let lp = ListParams::default().labels(&label_selector);
        let owned_pods = pods.list(&lp).await?;

        // Count pods by status
        let mut ready = 0;
        let mut busy = 0;
        let mut _pending = 0;

        for pod in &owned_pods {
            let phase = pod
                .status
                .as_ref()
                .and_then(|s| s.phase.as_deref())
                .unwrap_or("Unknown");

            match phase {
                "Running" => {
                    // Check if agent is busy via annotation
                    let is_busy = pod
                        .annotations()
                        .get("meticulous.dev/busy")
                        .map(|v| v == "true")
                        .unwrap_or(false);

                    if is_busy {
                        busy += 1;
                    } else {
                        ready += 1;
                    }
                }
                "Pending" => _pending += 1,
                _ => {}
            }
        }

        let current_count = owned_pods.items.len() as i32;
        let idle = ready;

        // Determine desired count
        let spec = &pool.spec;
        let desired =
            Self::calculate_desired_replicas(spec, ready, busy, idle, ctx.as_ref(), &name).await;

        // Scale up or down
        if current_count < desired {
            let to_create = desired - current_count;
            info!(
                current = current_count,
                desired,
                creating = to_create,
                "scaling up"
            );

            for i in 0..to_create {
                if let Err(e) = Self::create_agent_pod(&pool, &pods, i as usize).await {
                    error!(error = %e, "failed to create agent pod");
                }
            }
        } else if current_count > desired {
            let to_delete = current_count - desired;
            info!(
                current = current_count,
                desired,
                deleting = to_delete,
                "scaling down"
            );

            // Delete idle pods first
            let mut deleted = 0;
            for pod in &owned_pods {
                if deleted >= to_delete {
                    break;
                }

                let is_busy = pod
                    .annotations()
                    .get("meticulous.dev/busy")
                    .map(|v| v == "true")
                    .unwrap_or(false);

                if !is_busy {
                    let pod_name = pod.name_any();
                    info!(pod = %pod_name, "deleting idle agent pod");
                    if let Err(e) = pods.delete(&pod_name, &Default::default()).await {
                        warn!(error = %e, pod = %pod_name, "failed to delete pod");
                    } else {
                        deleted += 1;
                    }
                }
            }
        }

        // Update status
        let status = AgentPoolStatus {
            ready,
            busy,
            idle,
            total_jobs_completed: pool
                .status
                .as_ref()
                .map(|s| s.total_jobs_completed)
                .unwrap_or(0),
            last_scale_time: Some(chrono::Utc::now().to_rfc3339()),
            conditions: vec![PoolCondition {
                r#type: "Ready".to_string(),
                status: if ready > 0 { "True" } else { "False" }.to_string(),
                reason: "AgentsAvailable".to_string(),
                message: format!("{} agents ready", ready),
                last_transition_time: chrono::Utc::now().to_rfc3339(),
            }],
            observed_generation: pool.metadata.generation.unwrap_or(0),
        };

        let status_patch = serde_json::json!({
            "status": status
        });

        pools
            .patch_status(
                &name,
                &PatchParams::apply("met-operator"),
                &Patch::Merge(&status_patch),
            )
            .await?;

        // Requeue after 30 seconds
        Ok(Action::requeue(Duration::from_secs(30)))
    }

    /// Calculate the desired number of replicas.
    async fn calculate_desired_replicas(
        spec: &crate::crd::AgentPoolSpec,
        ready: i32,
        busy: i32,
        idle: i32,
        ctx: &Context,
        pool_name: &str,
    ) -> i32 {
        let min = spec.replicas.min;
        let max = spec.replicas.max.unwrap_or(100);
        let target_idle = spec.replicas.idle;

        // Basic scaling logic: maintain target idle count
        let current = ready + busy;
        let needed_for_idle = if idle < target_idle {
            target_idle - idle
        } else {
            0
        };

        let desired = current + needed_for_idle;

        // Check queue depth if NATS is available
        let queue_adjustment = if let Some(ref nats) = ctx.nats {
            Self::calculate_queue_adjustment(nats, spec, pool_name).await
        } else {
            0
        };

        let final_desired = (desired + queue_adjustment).clamp(min, max);

        debug!(
            current,
            ready,
            busy,
            idle,
            target_idle,
            queue_adjustment,
            desired,
            final_desired,
            "calculated desired replicas"
        );

        final_desired
    }

    /// Calculate scaling adjustment based on NATS queue depth.
    async fn calculate_queue_adjustment(
        nats: &async_nats::Client,
        spec: &crate::crd::AgentPoolSpec,
        pool_name: &str,
    ) -> i32 {
        let jetstream = async_nats::jetstream::new(nats.clone());

        // Get the JOBS stream to check pending messages
        let mut stream = match jetstream.get_stream("JOBS").await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "failed to get JOBS stream for queue depth check");
                return 0;
            }
        };

        // Get consumer info for this pool's job dispatch subject
        // Consumer name follows the pattern: pool-{org_id}-{pool_tag}
        // Since we don't have org_id here, we'll query stream info instead
        let stream_info = match stream.info().await {
            Ok(info) => info,
            Err(e) => {
                warn!(error = %e, "failed to get stream info");
                return 0;
            }
        };

        // Get total pending messages for the pool's subjects
        // Subject pattern: met.jobs.*.{pool_name}
        let pending_messages = stream_info.state.messages;

        // Calculate adjustment based on queue depth thresholds
        let autoscaler = spec.autoscaler.as_ref();

        // Default thresholds if not configured
        let scale_up_threshold = autoscaler
            .and_then(|a| a.queue_depth_scale_up_threshold)
            .unwrap_or(10) as u64;

        let scale_down_threshold = autoscaler
            .and_then(|a| a.queue_depth_scale_down_threshold)
            .unwrap_or(0) as u64;

        let scale_up_step = autoscaler.and_then(|a| a.scale_up_step).unwrap_or(1);

        let scale_down_step = autoscaler.and_then(|a| a.scale_down_step).unwrap_or(1);

        let adjustment = if pending_messages > scale_up_threshold {
            // Scale up: more jobs waiting than threshold
            let multiplier =
                ((pending_messages - scale_up_threshold) / scale_up_threshold.max(1) + 1) as i32;
            (scale_up_step * multiplier).min(10) // Cap at 10 per cycle
        } else if pending_messages <= scale_down_threshold {
            // Scale down: queue is empty or below threshold
            -scale_down_step
        } else {
            0
        };

        debug!(
            pool = %pool_name,
            pending_messages,
            scale_up_threshold,
            scale_down_threshold,
            adjustment,
            "calculated queue depth adjustment"
        );

        adjustment
    }

    /// Create a new agent pod.
    async fn create_agent_pod(pool: &AgentPool, pods: &Api<Pod>, index: usize) -> Result<()> {
        let name = pool.name_any();
        let namespace = pool.namespace().unwrap_or_default();
        let pod_name = format!("{}-agent-{}-{}", name, index, uuid::Uuid::new_v4().simple());

        // Build pod from template
        let mut pod = Pod {
            metadata: pool.spec.template.metadata.clone().unwrap_or_default(),
            spec: pool.spec.template.spec.clone(),
            ..Default::default()
        };

        // Set pod name and labels
        pod.metadata.name = Some(pod_name.clone());
        pod.metadata.namespace = Some(namespace);

        let labels = pod.metadata.labels.get_or_insert_with(Default::default);
        labels.insert("meticulous.dev/pool".to_string(), name.clone());
        labels.insert(
            "meticulous.dev/managed-by".to_string(),
            "met-operator".to_string(),
        );

        let annotations = pod
            .metadata
            .annotations
            .get_or_insert_with(Default::default);
        annotations.insert("meticulous.dev/busy".to_string(), "false".to_string());

        // Add owner reference
        if let Some(uid) = &pool.metadata.uid {
            pod.metadata.owner_references = Some(vec![
                k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference {
                    api_version: "meticulous.dev/v1alpha1".to_string(),
                    kind: "AgentPool".to_string(),
                    name: name.clone(),
                    uid: uid.clone(),
                    controller: Some(true),
                    block_owner_deletion: Some(true),
                },
            ]);
        }

        // Add environment variables for controller URL and join token
        if let Some(ref spec) = pod.spec {
            let mut spec = spec.clone();
            for container in &mut spec.containers {
                let env = container.env.get_or_insert_with(Default::default);

                env.push(k8s_openapi::api::core::v1::EnvVar {
                    name: "MET_CONTROLLER_URL".to_string(),
                    value: Some(pool.spec.controller_url.clone()),
                    value_from: None,
                });

                env.push(k8s_openapi::api::core::v1::EnvVar {
                    name: "MET_JOIN_TOKEN".to_string(),
                    value: None,
                    value_from: Some(k8s_openapi::api::core::v1::EnvVarSource {
                        secret_key_ref: Some(k8s_openapi::api::core::v1::SecretKeySelector {
                            name: pool.spec.join_token_secret_ref.name.clone(),
                            key: pool.spec.join_token_secret_ref.key.clone(),
                            optional: Some(false),
                        }),
                        ..Default::default()
                    }),
                });

                // Add pool tags
                if !pool.spec.pool_tags.is_empty() {
                    env.push(k8s_openapi::api::core::v1::EnvVar {
                        name: "MET_AGENT_TAGS".to_string(),
                        value: Some(pool.spec.pool_tags.join(",")),
                        value_from: None,
                    });
                }
            }
            pod.spec = Some(spec);
        }

        info!(pod = %pod_name, pool = %name, "creating agent pod");

        pods.create(&PostParams::default(), &pod).await?;

        Ok(())
    }

    /// Error policy for the controller.
    fn error_policy(pool: Arc<AgentPool>, error: &OperatorError, _ctx: Arc<Context>) -> Action {
        error!(
            pool = %pool.name_any(),
            error = %error,
            "reconciliation error"
        );
        Action::requeue(Duration::from_secs(60))
    }
}
