use crate::{
    PlatformStore, SecretGenerator, document_content_validation::DocumentContentValidation,
    document_policy_store::DocumentPolicyStore, document_version_checksum::DocumentVersionChecksum,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::Value;

pub(crate) struct DocumentRevisionStore;
impl DocumentRevisionStore {
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        id: &str,
        number: i64,
        previous: Option<&str>,
        input: &ProjectDocumentRevise,
        verified: &crate::document_verified_context::DocumentVerifiedContext,
    ) -> Result<ProjectDocumentVersion, PlatformError> {
        let title = DomainText::name(&input.title)?;
        let reason = DomainText::required(&input.reason, 4000)?;
        let content = DocumentContentValidation::validate(&input.content)?;
        // A declared origin is not code evidence. ObservedCode is reserved until anchors can be verified.
        if matches!(input.origin, DocumentOrigin::ObservedCode) && verified.evidence.is_empty() {
            return Err(PlatformError::Validation(
                "observed code requires verified repository evidence",
            ));
        }
        let normalized: Value = tx
            .query_one("SELECT $1::jsonb", &[&content])
            .map_err(PlatformStore::database_error)?
            .get(0);
        let typed: DocumentContent = PlatformStore::decode(normalized.clone())?;
        DocumentContentValidation::validate(&typed)?;
        let mut version = ProjectDocumentVersion {
            id: id.into(),
            project_id: project.into(),
            document_id: document.into(),
            number,
            previous_version_id: previous.map(String::from),
            title,
            content: normalized,
            origin: input.origin,
            workflow_version_id: None,
            state_id: "draft".into(),
            state_revision: 1,
            checksum: String::new(),
            reason,
            created_by: ctx.actor_id().into(),
            created_at: String::new(),
            context_manifest_id: verified.manifest_id.clone(),
            evidence: verified.evidence.clone(),
            needs_review: false,
        };
        version.checksum = DocumentVersionChecksum::calculate(&version)?;
        let origin = serde_json::to_value(version.origin).map_err(|_| PlatformError::Storage)?;
        let origin = origin.as_str().ok_or(PlatformError::Storage)?;
        tx.execute("INSERT INTO project_document_versions(id,project_id,document_id,number,previous_version_id,title,content,origin,workflow_version_id,checksum,reason,created_by,context_manifest_id,evidence) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",&[&version.id,&project,&document,&number,&version.previous_version_id,&version.title,&version.content,&origin,&version.workflow_version_id,&version.checksum,&version.reason,&ctx.actor_id(),&version.context_manifest_id,&serde_json::to_value(&version.evidence).map_err(|_|PlatformError::Storage)?]).map_err(PlatformStore::database_error)?;
        tx.execute(
            "INSERT INTO document_version_states(version_id,state_id) VALUES($1,$2)",
            &[&version.id, &version.state_id],
        )
        .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::version(tx, project, document, id)
    }
    pub fn next_number(document: &ProjectDocument) -> Result<i64, PlatformError> {
        document
            .latest_version_number
            .checked_add(1)
            .ok_or(PlatformError::Conflict("document version exhausted"))
    }
    pub fn id() -> Result<String, PlatformError> {
        SecretGenerator::id("dcv")
    }
}
