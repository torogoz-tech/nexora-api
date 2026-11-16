use crate::{
    PlatformStore, knowledge_contribution_policy::KnowledgeContributionPolicy,
    knowledge_contribution_store::KnowledgeContributionStore,
};
use nexora_platform::{AuthContext, KnowledgeContributionInput, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn write_knowledge_contribution(
        &mut self,
        ctx: &AuthContext,
        id: Option<&str>,
        input: &KnowledgeContributionInput,
        bytes: Option<&[u8]>,
    ) -> Result<Value, PlatformError> {
        KnowledgeContributionStore::write(self, ctx, id, input, bytes)
    }
    pub fn knowledge_contributions(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        kind: &str,
        search: &str,
        after: Option<&str>,
        descending: bool,
    ) -> Result<Value, PlatformError> {
        if !["document", "technical_debt"].contains(&kind)
            || search.len() > 256
            || after.is_some_and(|s| s.len() > 128)
        {
            return Err(PlatformError::Validation("invalid contribution query"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        KnowledgeContributionPolicy::read(&mut tx, ctx, project)?;
        let (direction, comparison) = if descending {
            ("DESC", "<")
        } else {
            ("ASC", ">")
        };
        let query = format!(
            "SELECT to_jsonb(c)-'payload'-'body' FROM knowledge_contributions c WHERE project_id IS NOT DISTINCT FROM $1 AND kind=$2 AND strpos(lower(title||' '||body),lower($3))>0 AND ($4::text IS NULL OR (lower(title),id) {comparison} (SELECT lower(a.title),a.id FROM knowledge_contributions a WHERE a.id=$4 AND a.project_id IS NOT DISTINCT FROM $1 AND a.kind=$2)) ORDER BY lower(title) {direction},id {direction} LIMIT 21"
        );
        let mut items: Vec<Value> = tx
            .query(&query, &[&project, &kind, &search, &after])
            .map_err(Self::database_error)?
            .into_iter()
            .map(|r| r.get(0))
            .collect();
        let more = items.len() > 20;
        items.truncate(20);
        let next = if more {
            items.last().and_then(|v| v["id"].as_str())
        } else {
            None
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items,"next_after":next}))
    }
    pub fn knowledge_contribution(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        revision: Option<i64>,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT project_id,to_jsonb(c)-'payload' FROM knowledge_contributions c WHERE id=$1 FOR SHARE",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        KnowledgeContributionPolicy::read(
            &mut tx,
            ctx,
            row.get::<_, Option<String>>(0).as_deref(),
        )?;
        let mut value: Value = row.get(1);
        if let Some(revision) = revision {
            let row=tx.query_opt("SELECT title,body,source_revision FROM knowledge_contribution_versions WHERE contribution_id=$1 AND revision=$2",&[&id,&revision]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
            value["title"] = json!(row.get::<_, String>(0));
            value["body"] = json!(row.get::<_, String>(1));
            value["revision"] = json!(revision);
            value["source_revision"] = json!(row.get::<_, i64>(2));
            if let Some(source)=tx.query_opt("SELECT filename,sha256 FROM knowledge_contribution_sources WHERE contribution_id=$1 AND source_revision=$2",&[&id,&row.get::<_,i64>(2)]).map_err(Self::database_error)?{value["filename"]=json!(source.get::<_,String>(0));value["sha256"]=json!(source.get::<_,String>(1));}
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn knowledge_contribution_file(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        revision: Option<i64>,
    ) -> Result<Vec<u8>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT c.project_id,s.payload,s.sha256 FROM knowledge_contributions c JOIN knowledge_contribution_sources s ON s.contribution_id=c.id AND s.source_revision=COALESCE((SELECT v.source_revision FROM knowledge_contribution_versions v WHERE v.contribution_id=c.id AND v.revision=$2),CASE WHEN $2::bigint IS NULL THEN c.source_revision END) WHERE c.id=$1 FOR SHARE OF c",&[&id,&revision]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        KnowledgeContributionPolicy::read(
            &mut tx,
            ctx,
            row.get::<_, Option<String>>(0).as_deref(),
        )?;
        let bytes: Vec<u8> = row.get(1);
        if crate::SecretGenerator::checksum(&bytes) != row.get::<_, String>(2) {
            return Err(PlatformError::Storage);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(bytes)
    }
}
