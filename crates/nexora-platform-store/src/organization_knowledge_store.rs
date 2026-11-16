use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    organization_knowledge_authority::OrganizationKnowledgeAuthority,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::{
    AuthContext, Capability, DomainText, OrganizationDocumentWrite, PlatformError,
};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn organization_knowledge_page(
        &mut self,
        ctx: &AuthContext,
        kind: &str,
        search: &str,
        after: Option<&str>,
    ) -> Result<Value, PlatformError> {
        if search.len() > 256 || after.is_some_and(|v| v.len() > 128) {
            return Err(PlatformError::Validation(
                "invalid organization knowledge query",
            ));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        OrganizationKnowledgeAuthority::read(&mut tx, ctx)?;
        let mut items:Vec<Value>=tx.query("SELECT to_jsonb(d)-'body' FROM organization_documents d WHERE kind=$1 AND (strpos(lower(title),lower($2))>0 OR strpos(lower(body),lower($2))>0) AND ($3::text IS NULL OR id>$3) ORDER BY id LIMIT 21",&[&kind,&search,&after]).map_err(Self::database_error)?.into_iter().map(|r|r.get(0)).collect();
        let more = items.len() > 20;
        items.truncate(20);
        let next = if more {
            items
                .last()
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            None
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items,"next_after":next}))
    }
    pub fn organization_knowledge_get(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        revision: Option<i64>,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        OrganizationKnowledgeAuthority::read(&mut tx, ctx)?;
        let value:Value=tx.query_opt("SELECT to_jsonb(v)||jsonb_build_object('id',d.id,'kind',d.kind,'published_revision',d.published_revision,'author_name',a.name) FROM organization_documents d JOIN organization_document_versions v ON v.document_id=d.id AND v.revision=COALESCE($2,d.revision) JOIN actors a ON a.id=v.created_by WHERE d.id=$1",&[&id,&revision]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn organization_knowledge_write(
        &mut self,
        ctx: &AuthContext,
        id: Option<&str>,
        input: &OrganizationDocumentWrite,
    ) -> Result<Value, PlatformError> {
        let title = DomainText::required(&input.title, 256)?;
        let body = DomainText::required(&input.body, 131072)?;
        if ![
            "document",
            "requirement",
            "business_rule",
            "decision",
            "best_practice",
            "security",
            "technical_debt",
        ]
        .contains(&input.kind.as_str())
        {
            return Err(PlatformError::Validation(
                "invalid organization document kind",
            ));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        OrganizationKnowledgeAuthority::read(&mut tx, ctx)?;
        // No project role grants implicit installation-scoped API-key authority.
        PlatformAuthority::require(&mut tx, ctx, Capability::OrganizationKnowledgeWrite, None)?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('technical_lead','pmo','manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Forbidden)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "organization.document.write",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let doc = if let Some(id) = id {
            id.to_owned()
        } else {
            SecretGenerator::id("odoc")?
        };
        if id.is_some() {
            let row = tx
                .query_opt(
                    "SELECT revision,kind FROM organization_documents WHERE id=$1 FOR UPDATE",
                    &[&doc],
                )
                .map_err(Self::database_error)?
                .ok_or(PlatformError::NotFound)?;
            if Some(row.get::<_, i64>(0)) != input.expected_revision {
                return Err(PlatformError::Conflict(
                    "organization document revision changed",
                ));
            }
            if row.get::<_, String>(1) != input.kind {
                return Err(PlatformError::Validation("document kind cannot change"));
            }
            tx.execute("UPDATE organization_documents SET title=$2,body=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1",&[&doc,&title,&body]).map_err(Self::database_error)?;
        } else {
            if input.expected_revision.is_some() {
                return Err(PlatformError::Validation(
                    "new document cannot have a revision",
                ));
            }
            tx.execute("INSERT INTO organization_documents(id,kind,title,body,created_by) VALUES($1,$2,$3,$4,$5)",&[&doc,&input.kind,&title,&body,&ctx.actor_id()]).map_err(Self::database_error)?;
        }
        tx.execute("INSERT INTO organization_document_versions(document_id,revision,title,body,created_by) SELECT id,revision,title,body,$2 FROM organization_documents WHERE id=$1",&[&doc,&ctx.actor_id()]).map_err(Self::database_error)?;
        let value: Value = tx
            .query_one(
                "SELECT to_jsonb(d) FROM organization_documents d WHERE id=$1",
                &[&doc],
            )
            .map_err(Self::database_error)?
            .get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "organization.document.write",
            Some(&doc),
            json!({"revision":value["revision"],"kind":input.kind}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
