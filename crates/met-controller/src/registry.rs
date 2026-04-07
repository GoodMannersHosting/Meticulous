//! In-memory agent registry with database backing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use met_core::ids::{AgentId, JobRunId, OrganizationId};
use met_core::models::{Agent, AgentStatus};
use tokio::sync::RwLock;

/// In-memory state for a registered agent.
#[derive(Debug, Clone)]
pub struct AgentState {
    /// Agent ID.
    pub agent_id: AgentId,
    /// Organization ID.
    pub org_id: OrganizationId,
    /// Current status.
    pub status: AgentStatus,
    /// Last heartbeat time (monotonic).
    pub last_heartbeat: Instant,
    /// Last heartbeat timestamp (wall clock).
    pub last_heartbeat_at: DateTime<Utc>,
    /// OS.
    pub os: String,
    /// Architecture.
    pub arch: String,
    /// Pool tags.
    pub pool_tags: Vec<String>,
    /// Labels.
    pub labels: Vec<String>,
    /// Maximum concurrent jobs.
    pub max_jobs: i32,
    /// Current running jobs.
    pub running_jobs: i32,
    /// Current job being executed (if any).
    pub current_job: Option<JobRunId>,
    /// JWT expiration time.
    pub jwt_expires_at: DateTime<Utc>,
    /// Resource snapshot (CPU, memory, disk).
    pub resources: Option<ResourceSnapshot>,
}

/// Resource utilization snapshot.
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub disk_percent: f32,
}

/// Build in-memory state from a persisted [`Agent`] row (e.g. after controller restart).
#[must_use]
pub fn agent_state_from_db_row(agent: &Agent) -> AgentState {
    let pool_tags = if !agent.pool_tags.is_empty() {
        agent.pool_tags.clone()
    } else if let Some(ref p) = agent.pool {
        vec![p.clone()]
    } else {
        vec!["_default".to_string()]
    };
    let labels = agent.tags.clone();
    let jwt_expires_at = agent
        .jwt_expires_at
        .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(24));

    AgentState {
        agent_id: agent.id,
        org_id: agent.org_id,
        status: agent.status,
        last_heartbeat: Instant::now(),
        last_heartbeat_at: agent.last_heartbeat_at.unwrap_or_else(Utc::now),
        os: agent.os.clone(),
        arch: agent.arch.clone(),
        pool_tags,
        labels,
        max_jobs: agent.max_jobs,
        running_jobs: agent.running_jobs,
        current_job: None,
        jwt_expires_at,
        resources: None,
    }
}

impl AgentState {
    /// Check if the agent can accept new jobs.
    pub fn can_accept_jobs(&self) -> bool {
        self.status == AgentStatus::Online && self.running_jobs < self.max_jobs
    }

    /// Check if the agent is stale (missed heartbeat threshold).
    pub fn is_stale(&self, threshold: std::time::Duration) -> bool {
        self.last_heartbeat.elapsed() > threshold
    }
}

/// Thread-safe agent registry.
#[derive(Clone)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<AgentId, AgentState>>>,
    by_org: Arc<RwLock<HashMap<OrganizationId, Vec<AgentId>>>>,
    by_pool: Arc<RwLock<HashMap<String, Vec<AgentId>>>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            by_org: Arc::new(RwLock::new(HashMap::new())),
            by_pool: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new agent.
    pub async fn register(&self, state: AgentState) {
        let agent_id = state.agent_id;
        let org_id = state.org_id;
        let pool_tags = state.pool_tags.clone();

        // Insert into main map
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent_id, state);
        }

        // Index by organization
        {
            let mut by_org = self.by_org.write().await;
            by_org.entry(org_id).or_default().push(agent_id);
        }

        // Index by pool tags
        {
            let mut by_pool = self.by_pool.write().await;
            for tag in pool_tags {
                by_pool.entry(tag).or_default().push(agent_id);
            }
        }
    }

    /// Insert an agent only if not already present (used to rehydrate after process restart).
    pub async fn register_if_missing(&self, state: AgentState) -> bool {
        let agent_id = state.agent_id;
        let org_id = state.org_id;
        let pool_tags = state.pool_tags.clone();

        {
            let mut agents = self.agents.write().await;
            if agents.contains_key(&agent_id) {
                return false;
            }
            agents.insert(agent_id, state);
        }

        {
            let mut by_org = self.by_org.write().await;
            by_org.entry(org_id).or_default().push(agent_id);
        }

        {
            let mut by_pool = self.by_pool.write().await;
            for tag in pool_tags {
                by_pool.entry(tag).or_default().push(agent_id);
            }
        }

        true
    }

    /// Get an agent by ID.
    pub async fn get(&self, agent_id: AgentId) -> Option<AgentState> {
        let agents = self.agents.read().await;
        agents.get(&agent_id).cloned()
    }

    /// Update agent heartbeat.
    pub async fn heartbeat(
        &self,
        agent_id: AgentId,
        status: AgentStatus,
        running_jobs: i32,
        current_job: Option<JobRunId>,
        resources: Option<ResourceSnapshot>,
    ) -> Option<AgentState> {
        let mut agents = self.agents.write().await;
        if let Some(state) = agents.get_mut(&agent_id) {
            state.last_heartbeat = Instant::now();
            state.last_heartbeat_at = Utc::now();
            state.status = status;
            state.running_jobs = running_jobs;
            state.current_job = current_job;
            state.resources = resources;
            Some(state.clone())
        } else {
            None
        }
    }

    /// Update agent status.
    pub async fn update_status(&self, agent_id: AgentId, status: AgentStatus) -> bool {
        let mut agents = self.agents.write().await;
        if let Some(state) = agents.get_mut(&agent_id) {
            state.status = status;
            true
        } else {
            false
        }
    }

    /// Remove an agent from the registry.
    pub async fn remove(&self, agent_id: AgentId) -> Option<AgentState> {
        let state = {
            let mut agents = self.agents.write().await;
            agents.remove(&agent_id)
        };

        if let Some(ref state) = state {
            // Remove from org index
            {
                let mut by_org = self.by_org.write().await;
                if let Some(agents) = by_org.get_mut(&state.org_id) {
                    agents.retain(|id| *id != agent_id);
                }
            }

            // Remove from pool index
            {
                let mut by_pool = self.by_pool.write().await;
                for tag in &state.pool_tags {
                    if let Some(agents) = by_pool.get_mut(tag) {
                        agents.retain(|id| *id != agent_id);
                    }
                }
            }
        }

        state
    }

    /// List agents in an organization.
    pub async fn list_by_org(&self, org_id: OrganizationId) -> Vec<AgentState> {
        let by_org = self.by_org.read().await;
        let agents = self.agents.read().await;

        by_org
            .get(&org_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| agents.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List agents with a specific pool tag.
    pub async fn list_by_pool(&self, pool_tag: &str) -> Vec<AgentState> {
        let by_pool = self.by_pool.read().await;
        let agents = self.agents.read().await;

        by_pool
            .get(pool_tag)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| agents.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List available agents matching requirements.
    pub async fn list_available(
        &self,
        org_id: OrganizationId,
        required_tags: &[String],
    ) -> Vec<AgentState> {
        let agents = self.agents.read().await;

        agents
            .values()
            .filter(|a| {
                a.org_id == org_id
                    && a.can_accept_jobs()
                    && required_tags.iter().all(|tag| a.pool_tags.contains(tag))
            })
            .cloned()
            .collect()
    }

    /// Find stale agents (exceeded threshold since last heartbeat).
    pub async fn find_stale(&self, threshold: std::time::Duration) -> Vec<AgentState> {
        let agents = self.agents.read().await;

        agents
            .values()
            .filter(|a| {
                a.status != AgentStatus::Offline
                    && a.status != AgentStatus::Dead
                    && a.is_stale(threshold)
            })
            .cloned()
            .collect()
    }

    /// Count agents by status.
    pub async fn count_by_status(&self) -> HashMap<AgentStatus, usize> {
        let agents = self.agents.read().await;

        let mut counts = HashMap::new();
        for agent in agents.values() {
            *counts.entry(agent.status).or_insert(0) += 1;
        }
        counts
    }

    /// Get total agent count.
    pub async fn total_count(&self) -> usize {
        let agents = self.agents.read().await;
        agents.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(agent_id: AgentId, org_id: OrganizationId) -> AgentState {
        AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now(),
            last_heartbeat_at: Utc::now(),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            pool_tags: vec!["default".to_string()],
            labels: vec![],
            max_jobs: 1,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
            resources: None,
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();
        let state = make_state(agent_id, org_id);

        registry.register(state.clone()).await;

        let retrieved = registry.get(agent_id).await.unwrap();
        assert_eq!(retrieved.agent_id, agent_id);
        assert_eq!(retrieved.org_id, org_id);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();
        let state = make_state(agent_id, org_id);

        registry.register(state).await;

        let updated = registry
            .heartbeat(agent_id, AgentStatus::Busy, 1, None, None)
            .await
            .unwrap();
        assert_eq!(updated.status, AgentStatus::Busy);
        assert_eq!(updated.running_jobs, 1);
    }

    #[tokio::test]
    async fn test_list_available() {
        let registry = AgentRegistry::new();
        let org_id = OrganizationId::new();

        // Register two agents
        let agent1 = AgentId::new();
        let mut state1 = make_state(agent1, org_id);
        state1.pool_tags = vec!["docker".to_string(), "linux".to_string()];
        registry.register(state1).await;

        let agent2 = AgentId::new();
        let mut state2 = make_state(agent2, org_id);
        state2.pool_tags = vec!["linux".to_string()];
        registry.register(state2).await;

        // Find agents with both tags
        let available = registry
            .list_available(org_id, &["docker".to_string(), "linux".to_string()])
            .await;
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].agent_id, agent1);

        // Find agents with just linux
        let available = registry
            .list_available(org_id, &["linux".to_string()])
            .await;
        assert_eq!(available.len(), 2);
    }

    #[tokio::test]
    async fn test_remove() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();
        let state = make_state(agent_id, org_id);

        registry.register(state).await;
        assert!(registry.get(agent_id).await.is_some());

        registry.remove(agent_id).await;
        assert!(registry.get(agent_id).await.is_none());
    }
}
