use crate::*;
use nexora_core::{RepositoryId, TenantId};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Component, Path, PathBuf},
};
pub struct FileFindingRepository {
    root: PathBuf,
}
impl FileFindingRepository {
    pub fn new(root: &Path) -> Result<Self, GovernanceError> {
        Self::directory(root)?;
        Ok(Self {
            root: GovernanceIo::path(root)?,
        })
    }
    fn directory(path: &Path) -> Result<(), GovernanceError> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut current = PathBuf::new();
        for component in absolute.components() {
            if matches!(component, Component::ParentDir) {
                return Err(GovernanceError::Scope(
                    "store parent traversal rejected".into(),
                ));
            }
            current.push(component.as_os_str());
            match std::fs::symlink_metadata(&current) {
                Ok(m) => {
                    if m.file_type().is_symlink() || !m.is_dir() {
                        return Err(GovernanceError::Scope(
                            "store path must contain regular directories".into(),
                        ));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&current)?
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn tenant_directory(&self, tenant: &TenantId) -> PathBuf {
        self.root
            .join(format!("{:x}", Sha256::digest(tenant.0.as_bytes())))
    }
    fn id(id: &str) -> Result<(), GovernanceError> {
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(GovernanceError::Configuration(
                "evaluation id must be a SHA-256 hex string".into(),
            ));
        }
        Ok(())
    }
    fn atomic(path: &Path, text: &[u8]) -> Result<(), GovernanceError> {
        if let Ok(m) = std::fs::symlink_metadata(path)
            && (!m.is_file() || m.file_type().is_symlink())
        {
            return Err(GovernanceError::Scope(
                "report target is not a regular file".into(),
            ));
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| GovernanceError::Configuration("system clock".into()))?
            .as_nanos();
        let temporary = path.with_extension(format!("tmp-{}-{nonce}", std::process::id()));
        let result = (|| -> Result<(), GovernanceError> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(text)?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
    pub fn latest(
        &self,
        tenant: &TenantId,
    ) -> Result<Option<EvaluationReference>, GovernanceError> {
        let path = self.tenant_directory(tenant).join("latest.json");
        if !path.try_exists()? {
            return Ok(None);
        }
        let reference: EvaluationReference =
            serde_json::from_str(&GovernanceIo::read(&path, 4096)?)?;
        if &reference.tenant_id != tenant {
            return Err(GovernanceError::Scope(
                "latest evaluation belongs to another tenant".into(),
            ));
        }
        Self::id(&reference.evaluation_id)?;
        Ok(Some(reference))
    }
}
impl FindingRepository for FileFindingRepository {
    fn save(&mut self, result: &EvaluationResult) -> Result<(), GovernanceError> {
        GovernanceReporter::verify(result)?;
        Self::id(&result.id)?;
        let directory = self.tenant_directory(&result.tenant_id);
        Self::directory(&directory)?;
        let bytes = serde_json::to_vec_pretty(result)?;
        if bytes.len() > 8_388_608 {
            return Err(GovernanceError::Limit("report > 8 MiB".into()));
        }
        Self::atomic(&directory.join(format!("{}.json", result.id)), &bytes)?;
        let reference = EvaluationReference {
            tenant_id: result.tenant_id.clone(),
            repository_id: result.repository_id.clone(),
            evaluation_id: result.id.clone(),
        };
        Self::atomic(
            &directory.join("latest.json"),
            &serde_json::to_vec(&reference)?,
        )?;
        Ok(())
    }
    fn get(
        &self,
        tenant: &TenantId,
        repository: &RepositoryId,
        evaluation: &str,
    ) -> Result<Option<EvaluationResult>, GovernanceError> {
        Self::id(evaluation)?;
        let path = self
            .tenant_directory(tenant)
            .join(format!("{evaluation}.json"));
        if !path.try_exists()? {
            return Ok(None);
        }
        let value: EvaluationResult = serde_json::from_str(&GovernanceIo::read(&path, 8_388_608)?)?;
        if &value.tenant_id != tenant
            || &value.repository_id != repository
            || value.id != evaluation
        {
            return Err(GovernanceError::Scope(
                "evaluation is outside requested scope".into(),
            ));
        }
        GovernanceReporter::verify(&value)?;
        Ok(Some(value))
    }
}
