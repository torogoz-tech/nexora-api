use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct KnowledgeReviewPolicy;
impl KnowledgeReviewPolicy {
    pub fn snapshot(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        kind: &str,
        id: &str,
        write: bool,
    ) -> Result<Value, PlatformError> {
        let sql = match kind {
            "project_document" => {
                "SELECT jsonb_build_object('project_id',d.project_id,'revision',d.revision,'version_id',v.id,'title',v.title,'document_kind',d.kind) FROM project_documents d JOIN project_document_versions v ON v.id=d.latest_version_id WHERE d.id=$1 AND NOT d.archived FOR UPDATE OF d"
            }
            "organization_document" => {
                "SELECT jsonb_build_object('project_id',NULL,'revision',revision,'title',title,'document_kind',kind) FROM organization_documents WHERE id=$1 FOR UPDATE"
            }
            "contribution" => {
                "SELECT jsonb_build_object('project_id',project_id,'revision',revision,'title',title,'document_kind',kind) FROM knowledge_contributions WHERE id=$1 AND status='ready' FOR UPDATE"
            }
            "feature" => {
                "SELECT jsonb_build_object('project_id',project_id,'revision',revision,'title',name,'document_kind','feature') FROM project_features WHERE id=$1 AND NOT archived FOR UPDATE"
            }
            _ => return Err(PlatformError::Validation("invalid knowledge review kind")),
        };
        let value: Value = tx
            .query_opt(sql, &[&id])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let project = value["project_id"].as_str();
        let capability = if write && kind == "contribution" {
            Capability::KnowledgeContribute
        } else {
            match (project, write) {
                (Some(_), true) => Capability::DocumentsWrite,
                (Some(_), false) => Capability::DocumentsRead,
                (None, true) => Capability::OrganizationKnowledgeWrite,
                (None, false) => Capability::OrganizationKnowledgeRead,
            }
        };
        PlatformAuthority::require(tx, ctx, capability, project)?;
        if let Some(project) = project {
            PlatformAuthority::project(tx, project)?;
        }
        Ok(value)
    }
    pub fn human(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        if let Some(project) = project {
            return crate::feature_governance_policy::FeatureGovernancePolicy::human(
                tx, ctx, project,
            );
        }
        PlatformAuthority::require(tx, ctx, Capability::OrganizationKnowledgeWrite, None)?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('technical_lead','pmo','manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        Ok(())
    }
    pub fn publication(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        kind: &str,
        id: &str,
        snapshot: &Value,
    ) -> Result<(), PlatformError> {
        match kind {
            "organization_document" => {
                tx.execute(
                    "UPDATE organization_documents SET published_revision=$2 WHERE id=$1",
                    &[&id, &snapshot["revision"].as_i64()],
                )
                .map_err(PlatformStore::database_error)?;
            }
            "project_document" => {
                let project = snapshot["project_id"]
                    .as_str()
                    .ok_or(PlatformError::NotFound)?;
                let version_id = snapshot["version_id"]
                    .as_str()
                    .ok_or(PlatformError::NotFound)?;
                let version = crate::document_policy_store::DocumentPolicyStore::version(
                    tx, project, id, version_id,
                )?;
                let publication = crate::SecretGenerator::id("dcp")?;
                tx.execute("INSERT INTO document_publications(id,project_id,document_id,version_id,checksum,workflow_version_id,state_id,state_revision,published_by,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'Aprobación de conocimiento') ON CONFLICT(version_id,state_revision) DO NOTHING",&[&publication,&project,&id,&version.id,&version.checksum,&version.workflow_version_id,&version.state_id,&version.state_revision,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
                tx.execute("UPDATE project_documents SET published_version_id=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1",&[&id,&version.id]).map_err(PlatformStore::database_error)?;
                crate::document_history_writer::DocumentHistoryWriter::append(
                    tx,
                    ctx,
                    project,
                    id,
                    &version.id,
                    "document.published",
                    "Aprobación de conocimiento",
                    json!({"publication_id":publication,"checksum":version.checksum}),
                )?;
            }
            _ => {}
        }
        Ok(())
    }
}
