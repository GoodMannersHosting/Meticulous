//! S3 presigned URLs for passive workspace snapshots (`met-engine` trait impl).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use met_core::ids::OrganizationId;
use met_engine::{EngineError, Result, WorkspaceSnapshotPresigner};
use met_objstore::{ObjectKey, ObjectStore};

/// Wraps [`ObjectStore`] for engine snapshot presigning.
pub struct S3WorkspaceSnapshotPresigner {
    store: Arc<dyn ObjectStore + Send + Sync>,
}

impl S3WorkspaceSnapshotPresigner {
    pub fn new(store: Arc<dyn ObjectStore + Send + Sync>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl WorkspaceSnapshotPresigner for S3WorkspaceSnapshotPresigner {
    async fn presign_put(
        &self,
        _org_id: OrganizationId,
        object_key: &str,
        expires_in: Duration,
    ) -> Result<String> {
        let url = self
            .store
            .presigned_put(&ObjectKey::new(object_key), expires_in)
            .await
            .map_err(|e| EngineError::internal(e.to_string()))?;
        Ok(url.to_string())
    }

    async fn presign_get(
        &self,
        _org_id: OrganizationId,
        object_key: &str,
        expires_in: Duration,
    ) -> Result<String> {
        let url = self
            .store
            .presigned_get(&ObjectKey::new(object_key), expires_in)
            .await
            .map_err(|e| EngineError::internal(e.to_string()))?;
        Ok(url.to_string())
    }
}
