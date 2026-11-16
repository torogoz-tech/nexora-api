use crate::{
    PlatformStore, RepositoryJobLease,
    repository_job_execution_authority::RepositoryJobExecutionAuthority,
};
use nexora_language::AnalysisProfilePackage;
use nexora_platform::{PlatformError, RepositoryAnalysisProfileChange};
use postgres::Transaction;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) struct AnalysisProfileSelectionStore;
impl AnalysisProfileSelectionStore {
    pub fn configuration(
        tx: &mut Transaction<'_>,
        input: &RepositoryAnalysisProfileChange,
    ) -> Result<(String, Vec<String>), PlatformError> {
        let (mode, mut ids) = match input.mode.as_deref() {
            None => {
                if input.profile_version_ids.is_some() {
                    return Err(PlatformError::Validation(
                        "profile list requires an explicit selection mode",
                    ));
                }
                (
                    if input.profile_version_id.is_some() {
                        "manual"
                    } else {
                        "legacy"
                    },
                    input.profile_version_id.clone().into_iter().collect(),
                )
            }
            Some("automatic") => {
                if input.profile_version_id.is_some()
                    || input
                        .profile_version_ids
                        .as_ref()
                        .is_some_and(|ids| !ids.is_empty())
                {
                    return Err(PlatformError::Validation(
                        "automatic selection does not accept fixed profiles",
                    ));
                }
                ("automatic", vec![])
            }
            Some("manual") => {
                if input.profile_version_id.is_some() {
                    return Err(PlatformError::Validation(
                        "manual selection uses profile_version_ids",
                    ));
                }
                (
                    "manual",
                    input.profile_version_ids.clone().unwrap_or_default(),
                )
            }
            _ => {
                return Err(PlatformError::Validation(
                    "invalid analysis profile selection mode",
                ));
            }
        };
        if mode == "manual" && (ids.is_empty() || ids.len() > 64) {
            return Err(PlatformError::Validation(
                "manual selection requires between 1 and 64 profiles",
            ));
        }
        let mut extensions = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for id in &ids {
            if !seen.insert(id) {
                return Err(PlatformError::Validation("duplicate analysis profile"));
            }
            let row = tx
                .query_opt(
                    "SELECT package FROM analysis_profile_versions WHERE id=$1",
                    &[id],
                )
                .map_err(PlatformStore::database_error)?
                .ok_or(PlatformError::NotFound)?;
            let package: AnalysisProfilePackage = PlatformStore::decode(row.get(0))?;
            let root = package
                .models
                .iter()
                .find(|m| m.manifest.id.0 == package.language_id)
                .ok_or(PlatformError::Storage)?;
            for extension in &root.manifest.extensions {
                if !extensions.insert(extension.to_lowercase()) {
                    return Err(PlatformError::Validation(
                        "selected profiles overlap on a file extension",
                    ));
                }
            }
        }
        ids.sort();
        Ok((mode.into(), ids))
    }
    pub fn load(
        store: &mut PlatformStore,
        lease: &RepositoryJobLease,
    ) -> Result<Option<Value>, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let row = RepositoryJobExecutionAuthority::lock(&mut tx, lease, false)?;
        let value: Value = row.get(0);
        let plan = &value["analysis_profile_plan"];
        if plan.is_null() {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(None);
        }
        let ids = plan["profile_version_ids"]
            .as_array()
            .ok_or(PlatformError::Storage)?;
        if ids.len() > 1024 {
            return Err(PlatformError::Storage);
        }
        let mut candidates = Vec::new();
        let mut bytes = 0usize;
        for id in ids {
            let id = id.as_str().ok_or(PlatformError::Storage)?;
            let row=tx.query_opt("SELECT checksum,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),package FROM analysis_profile_versions WHERE id=$1", &[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Storage)?;
            let package: AnalysisProfilePackage = PlatformStore::decode(row.get(2))?;
            let checksum: String = row.get(0);
            if package.checksum().map_err(|_| PlatformError::Storage)? != checksum {
                return Err(PlatformError::Storage);
            }
            let package = serde_json::to_value(package).map_err(|_| PlatformError::Storage)?;
            bytes = bytes
                .checked_add(
                    serde_json::to_vec(&package)
                        .map_err(|_| PlatformError::Storage)?
                        .len(),
                )
                .ok_or(PlatformError::Storage)?;
            if bytes > 67_108_864 {
                return Err(PlatformError::Storage);
            }
            candidates.push(json!({"id":id,"checksum":checksum,"created_at":row.get::<_,String>(1),"package":package}));
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(Some(
            json!({"mode":plan["mode"],"policy":plan["policy"],"candidates":candidates}),
        ))
    }
}
