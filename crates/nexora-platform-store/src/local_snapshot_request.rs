use crate::SecretGenerator;
use nexora_platform::{LocalSnapshotAdoption, PlatformError};
use serde_json::{Value, json};

/// Canonical operator request identity; roots never enter receipts or audit metadata.
pub struct LocalSnapshotRequest;
impl LocalSnapshotRequest {
    pub fn canonical(project: &str, input: &LocalSnapshotAdoption) -> Result<Value, PlatformError> {
        input.validate()?;
        let root = input
            .root
            .canonicalize()
            .map_err(|_| PlatformError::Validation("local snapshot root is unavailable"))?;
        if !root.is_dir() {
            return Err(PlatformError::Validation(
                "local snapshot root must be a directory",
            ));
        }
        let mut value = serde_json::to_value(input).map_err(|_| PlatformError::Storage)?;
        let fields = value.as_object_mut().ok_or(PlatformError::Storage)?;
        fields.remove("root");
        fields.insert(
            "root_digest".into(),
            json!(SecretGenerator::checksum(root.to_string_lossy().as_bytes())),
        );
        Ok(json!({"project_id":project,"adoption":value}))
    }
}
