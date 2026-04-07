//! Build [`WorkflowInvocationOutputs`] from aggregated `met-output` IPC bytes.

use met_proto::controller::v1::{JobDispatch, WorkflowInvocationOutputs, WorkflowSecretEnvelope};

use crate::error::{AgentError, Result};
use crate::output_drain::{decode_output_bytes, DrainError};
use crate::output_seal::seal_secret_value;

pub fn build_workflow_invocation_outputs(
    job: &JobDispatch,
    ipc_concat: &[u8],
) -> Result<Option<WorkflowInvocationOutputs>> {
    let inv = job.workflow_invocation_id.trim();
    if inv.is_empty() {
        return Ok(None);
    }

    if ipc_concat.is_empty() {
        return Ok(Some(WorkflowInvocationOutputs {
            workflow_invocation_id: inv.to_string(),
            public: Default::default(),
            secrets: vec![],
        }));
    }

    let drained = decode_output_bytes(ipc_concat).map_err(|e| match e {
        DrainError::MalformedFrame => AgentError::Internal("met-output: malformed IPC".into()),
        DrainError::AggregateLimit => AgentError::Internal("met-output: aggregate size exceeded".into()),
    })?;

    let pk: [u8; 32] = job
        .output_wrap_x25519_public_key
        .as_slice()
        .try_into()
        .map_err(|_| AgentError::Internal("job dispatch: bad output_wrap public key length".into()))?;

    if pk == [0u8; 32] && !drained.secret_plain.is_empty() {
        return Err(AgentError::Internal(
            "met-output secret values present but output wrap key is zeroed".into(),
        ));
    }

    let mut secrets = Vec::new();
    for (name, plain) in drained.secret_plain {
        let packed = seal_secret_value(&pk, &plain).map_err(|_| {
            AgentError::Internal("failed to seal workflow secret output".into())
        })?;
        if packed.len() < 44 {
            return Err(AgentError::Internal("internal: short sealed blob".into()));
        }
        secrets.push(WorkflowSecretEnvelope {
            name,
            ephemeral_x25519_public: packed[..32].to_vec(),
            nonce: packed[32..44].to_vec(),
            ciphertext: packed[44..].to_vec(),
        });
    }

    Ok(Some(WorkflowInvocationOutputs {
        workflow_invocation_id: inv.to_string(),
        public: drained.public.into_iter().collect(),
        secrets,
    }))
}
