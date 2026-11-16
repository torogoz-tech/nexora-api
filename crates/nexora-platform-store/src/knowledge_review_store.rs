use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    knowledge_review_policy::KnowledgeReviewPolicy as Policy, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn request_knowledge_review(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let kind = input["kind"]
            .as_str()
            .ok_or(PlatformError::Validation("review kind required"))?;
        let id = input["resource_id"]
            .as_str()
            .ok_or(PlatformError::Validation("knowledge id required"))?;
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "knowledge.review.request",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let target = Policy::snapshot(&mut tx, ctx, kind, id, true)?;
        let revision = target["revision"].as_i64().ok_or(PlatformError::NotFound)?;
        if input["expected_revision"].as_i64() != Some(revision) {
            return Err(PlatformError::Conflict(
                "knowledge changed; review the current version",
            ));
        }
        let review = SecretGenerator::id("krv")?;
        let value:Value=tx.query_one("INSERT INTO knowledge_reviews AS r(id,kind,resource_id,project_id,resource_revision,version_id,title,document_kind,requested_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(kind,resource_id,resource_revision) WHERE status='pending' DO UPDATE SET id=r.id RETURNING to_jsonb(r)",&[&review,&kind,&id,&target["project_id"].as_str(),&revision,&target["version_id"].as_str(),&target["title"].as_str(),&target["document_kind"].as_str(),&ctx.actor_id()]).map_err(Self::database_error)?.get(0);
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn knowledge_review(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let mut value: Value = tx
            .query_opt(
                "SELECT to_jsonb(r) FROM knowledge_reviews r WHERE id=$1",
                &[&id],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let target = Policy::snapshot(
            &mut tx,
            ctx,
            value["kind"].as_str().unwrap_or(""),
            value["resource_id"].as_str().unwrap_or(""),
            false,
        )?;
        value["current_revision"] = target["revision"].clone();
        value["can_decide"] =
            json!(Policy::human(&mut tx, ctx, value["project_id"].as_str()).is_ok());
        if let Some(job) = tx
            .query_opt(
                "SELECT to_jsonb(j) FROM knowledge_processing_jobs j WHERE j.source_kind=$1 AND j.resource_id=$2 AND j.source_revision=$3 ORDER BY j.created_at DESC LIMIT 1",
                &[
                    &value["kind"].as_str().unwrap_or(""),
                    &value["resource_id"].as_str().unwrap_or(""),
                    &value["resource_revision"].as_i64().unwrap_or(0),
                ],
            )
            .map_err(Self::database_error)?
        {
            value["processing"] = job.get(0);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn knowledge_reviews(
        &mut self,
        ctx: &AuthContext,
        resource: Option<&str>,
        status: &str,
        page: i64,
    ) -> Result<Value, PlatformError> {
        if !(1..=100000).contains(&page)
            || !matches!(status, "pending" | "approved" | "rejected" | "")
        {
            return Err(PlatformError::Validation("invalid review query"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let projects:Vec<String>=tx.query("SELECT DISTINCT p.id FROM projects p JOIN actor_grants g ON g.project_id IS NULL OR g.project_id=p.id WHERE g.actor_id=$1 AND g.capability='documents:read' AND NOT p.archived ORDER BY p.id",&[&ctx.actor_id()]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
        let mut allowed = Vec::new();
        for project in projects {
            match crate::platform_authority::PlatformAuthority::require(
                &mut tx,
                ctx,
                Capability::DocumentsRead,
                Some(&project),
            ) {
                Ok(()) => allowed.push(project),
                Err(PlatformError::Forbidden) => {}
                Err(e) => return Err(e),
            }
        }
        let general = match crate::platform_authority::PlatformAuthority::require(
            &mut tx,
            ctx,
            Capability::OrganizationKnowledgeRead,
            None,
        ) {
            Ok(()) => true,
            Err(PlatformError::Forbidden) => false,
            Err(e) => return Err(e),
        };
        if allowed.is_empty() && !general {
            return Err(PlatformError::Forbidden);
        }
        let mut value: Value = tx
            .query_one(
                include_str!("knowledge_review_queue.sql"),
                &[&status, &resource, &allowed, &general, &((page - 1) * 20)],
            )
            .map_err(Self::database_error)?
            .get(0);
        if let Some(items) = value["items"].as_array_mut() {
            for item in items {
                item["can_decide"] =
                    json!(Policy::human(&mut tx, ctx, item["project_id"].as_str()).is_ok());
            }
        }
        value["page"] = json!(page);
        value["page_size"] = json!(20);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn decide_knowledge_review(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let decision = input["decision"]
            .as_str()
            .ok_or(PlatformError::Validation("review decision required"))?;
        if !matches!(decision, "approved" | "rejected") {
            return Err(PlatformError::Validation("invalid review decision"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let review: Value = tx
            .query_opt(
                "SELECT to_jsonb(r) FROM knowledge_reviews r WHERE id=$1",
                &[&id],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        Policy::human(&mut tx, ctx, review["project_id"].as_str())?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "knowledge.review.decide",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        if review["status"] != "pending" || review["revision"] != input["expected_revision"] {
            return Err(PlatformError::Conflict("review decision changed"));
        }
        let kind = review["kind"].as_str().unwrap_or("");
        let resource = review["resource_id"].as_str().unwrap_or("");
        let target = if decision == "approved" {
            Policy::snapshot(&mut tx, ctx, kind, resource, false)?
        } else {
            crate::platform_authority::PlatformAuthority::require(
                &mut tx,
                ctx,
                if review["project_id"].is_string() {
                    Capability::DocumentsRead
                } else {
                    Capability::OrganizationKnowledgeRead
                },
                review["project_id"].as_str(),
            )?;
            Value::Null
        };
        if decision == "approved"
            && (target["revision"] != review["resource_revision"]
                || target["version_id"] != review["version_id"])
        {
            return Err(PlatformError::Conflict(
                "knowledge changed; request review of the current version",
            ));
        }
        let locked: Value = tx
            .query_one(
                "SELECT to_jsonb(r) FROM knowledge_reviews r WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .map_err(Self::database_error)?
            .get(0);
        if locked["status"] != "pending" || locked["revision"] != input["expected_revision"] {
            return Err(PlatformError::Conflict("review decision changed"));
        }
        if decision == "approved" {
            Policy::publication(&mut tx, ctx, kind, resource, &target)?;
        }
        let reason = DomainText::optional(input["reason"].as_str().unwrap_or(""), 4000)?;
        let mut value:Value=tx.query_one("UPDATE knowledge_reviews AS r SET status=$2,decided_by=$3,decided_at=clock_timestamp(),reason=$4,revision=revision+1 WHERE id=$1 RETURNING to_jsonb(r)",&[&id,&decision,&ctx.actor_id(),&reason]).map_err(Self::database_error)?.get(0);
        if decision == "approved" {
            let job = Self::enqueue_knowledge_processing(&mut tx, kind, resource, &target)?;
            value["processing_job"] = job;
        } else {
            Self::cancel_knowledge_processing(
                &mut tx,
                kind,
                resource,
                review["resource_revision"]
                    .as_i64()
                    .ok_or(PlatformError::Storage)?,
            )?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "knowledge.review.decide",
            Some(id),
            value.clone(),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
