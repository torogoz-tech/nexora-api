use crate::{
    PlatformStore, knowledge_contribution_policy::KnowledgeContributionPolicy,
    organization_knowledge_authority::OrganizationKnowledgeAuthority,
};
use nexora_platform::{AuthContext, DomainText, PlatformError};
use serde_json::{Value, json};

impl PlatformStore {
    pub fn authorize_knowledge_context_search(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        KnowledgeContributionPolicy::read(&mut tx, ctx, project)?;
        if project.is_some() {
            match OrganizationKnowledgeAuthority::read(&mut tx, ctx) {
                Ok(()) | Err(PlatformError::Forbidden) => {}
                Err(error) => return Err(error),
            }
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(())
    }

    /// Retrieve current documentation as untrusted evidence, never as agent instructions.
    pub fn knowledge_context_search(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        search: &str,
    ) -> Result<Value, PlatformError> {
        self.knowledge_context_search_with_embedding(ctx, project, search, None, None)
    }

    pub fn knowledge_context_search_with_embedding(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        search: &str,
        embedding: Option<&[f32]>,
        embedding_model: Option<&str>,
    ) -> Result<Value, PlatformError> {
        let search = DomainText::required(search, 256)?;
        let embedding_literal = embedding.map(|values| {
            format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        });
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        KnowledgeContributionPolicy::read(&mut tx, ctx, project)?;
        let organization_included = if project.is_none() {
            true
        } else {
            match OrganizationKnowledgeAuthority::read(&mut tx, ctx) {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(error) => return Err(error),
            }
        };
        let rows = tx
            .query(
                include_str!("knowledge_context_search.sql"),
                &[
                    &project,
                    &organization_included,
                    &search,
                    &embedding_literal,
                    &embedding_model,
                ],
            )
            .map_err(Self::database_error)?;
        let mut items: Vec<Value> = rows.into_iter().map(|row| row.get(0)).collect();
        let has_more = items.len() > 20;
        items.truncate(20);
        tx.commit().map_err(Self::database_error)?;
        Ok(
            json!({"items":items,"has_more":has_more,"organization_included":organization_included,"retrieval":"hybrid_rag","semantic_available":embedding.is_some(),"untrusted_content":true}),
        )
    }
}
