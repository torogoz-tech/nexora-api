use crate::{PlatformStore, SecretGenerator, WebhookProof};
use nexora_platform::PlatformError;
use serde_json::Value;
pub(crate) struct SourceEventWriter;
impl SourceEventWriter {
    pub fn source(value: &Value) -> Result<(&str, &str), PlatformError> {
        let sha = value["after"]
            .as_str()
            .filter(|v| [40, 64].contains(&v.len()) && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(PlatformError::Validation("invalid source commit"))?;
        let source_ref = value["ref"]
            .as_str()
            .filter(|v| {
                v.starts_with("refs/") && v.len() <= 512 && !v.chars().any(char::is_control)
            })
            .ok_or(PlatformError::Validation("invalid source ref"))?;
        Ok((sha, source_ref))
    }
    pub fn changed(
        client: &mut impl postgres::GenericClient,
        repository: &str,
        provider: &str,
        scope: &str,
        proof: &WebhookProof,
        source: (&str, &str),
    ) -> Result<(), PlatformError> {
        // Git branch/tag deletion has no commit to analyze.
        if source.0.bytes().all(|c| c == b'0') {
            return Ok(());
        }
        let commit = source.0.to_ascii_lowercase();
        let row=client.query_opt("UPDATE project_repositories SET source_generation=source_generation+1,latest_source_commit=$2,source_changed_at=clock_timestamp() WHERE id=$1 AND authorization_status='authorized' AND NOT archived RETURNING source_generation",&[&repository,&commit]).map_err(PlatformStore::database_error)?;
        if let Some(row) = row {
            let generation: i64 = row.get(0);
            let event = SecretGenerator::id("sev")?;
            client.execute("INSERT INTO repository_source_events(id,repository_id,source_generation,provider,scope_id,delivery_id,source_ref,source_commit) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[&event,&repository,&generation,&provider,&scope,&proof.delivery_id,&source.1,&commit]).map_err(PlatformStore::database_error)?;
            crate::repository_webhook_analysis_store::RepositoryWebhookAnalysisStore::record(
                client, &event, repository,
            )?;
        }
        Ok(())
    }
}
