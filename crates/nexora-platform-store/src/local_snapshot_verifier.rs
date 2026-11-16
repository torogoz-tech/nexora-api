use crate::{PlatformStore, SecretGenerator};
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::{LocalSnapshotAdoption, PlatformError};
use nexora_runtime::{DatabaseRuntime, QuerySnapshot};

pub(crate) struct LocalSnapshotVerifier;
impl LocalSnapshotVerifier {
    pub fn verify(
        store: &mut PlatformStore,
        input: &LocalSnapshotAdoption,
    ) -> Result<QuerySnapshot, PlatformError> {
        let root = input
            .root
            .canonicalize()
            .map_err(|_| PlatformError::Validation("local snapshot root is unavailable"))?;
        if !root.is_dir() {
            return Err(PlatformError::Validation(
                "local snapshot root must be a directory",
            ));
        }
        let repository = RepositoryId(SecretGenerator::checksum(root.to_string_lossy().as_bytes()));
        if store.code_runtime.is_none() {
            store.code_runtime = Some(
                DatabaseRuntime::connect(&store.connections).map_err(|_| PlatformError::Storage)?,
            );
        }
        store
            .code_runtime
            .as_mut()
            .ok_or(PlatformError::Storage)?
            .verify_persisted_snapshot(
                &TenantId(input.runtime_tenant_id.clone()),
                &repository,
                &SnapshotId(input.snapshot_id.clone()),
                &input.expected_payload_hash,
            )
            .map_err(|_| {
                PlatformError::Conflict(
                    "local snapshot integrity or projection verification failed",
                )
            })
    }
}
