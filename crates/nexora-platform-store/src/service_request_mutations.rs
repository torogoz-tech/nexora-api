use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
    service_request_policy::ServiceRequestPolicy as Policy,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn mutate_service_request(
        &mut self,
        ctx: &AuthContext,
        id: Option<&str>,
        action: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::platform_authority::PlatformAuthority::require(
            &mut tx,
            ctx,
            Capability::RequestsWrite,
            None,
        )?;
        let role = if let Some(id) = id {
            Policy::visible(&mut tx, ctx, id, true)?
        } else {
            Policy::role(&mut tx, ctx)?
        };
        let op = format!("service_request.{action}");
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            &op,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        };
        let resource;
        let detail = match action {
            "create" => {
                let title = Policy::text(input, "title", 300)?;
                let body = match input.get("body") {
                    None => "",
                    Some(value) => value
                        .as_str()
                        .ok_or(PlatformError::Validation("request content must be text"))?,
                };
                if body.len() > 32000 {
                    return Err(PlatformError::Validation("request content too long"));
                }
                let type_id = Policy::text(input, "type_id", 120)?;
                tx.query_opt(
                    "SELECT id FROM service_request_types WHERE id=$1 AND NOT archived FOR SHARE",
                    &[&type_id],
                )
                .map_err(Self::database_error)?
                .ok_or(PlatformError::Validation("request type unavailable"))?;
                let product = input["product_id"].as_str();
                if input
                    .get("product_id")
                    .is_some_and(|v| !v.is_null() && !v.is_string())
                {
                    return Err(PlatformError::Validation("invalid product"));
                }
                if let Some(product) = product {
                    tx.query_opt(
                        "SELECT id FROM products WHERE id=$1 AND NOT archived FOR SHARE",
                        &[&product],
                    )
                    .map_err(Self::database_error)?
                    .ok_or(PlatformError::Validation("product unavailable"))?;
                }
                let criticality = Policy::criticality(input)?;
                resource = SecretGenerator::id("srq")?;
                tx.execute("INSERT INTO service_requests(id,requester_id,type_id,title,body,product_id,criticality) VALUES($1,$2,$3,$4,$5,$6,$7)",&[&resource,&ctx.actor_id(),&type_id,&title,&body,&product,&criticality]).map_err(Self::database_error)?;
                tx.execute("UPDATE service_requests SET sla_snapshot=request_sla_snapshot(type_id,criticality,created_at) WHERE id=$1",&[&resource]).map_err(Self::database_error)?;
                json!({"criticality":criticality,"status":"new","type_id":type_id,"product_id":product})
            }
            "change" => {
                if !Policy::internal(&role) {
                    return Err(PlatformError::Forbidden);
                }
                resource = id.ok_or(PlatformError::NotFound)?.to_owned();
                let revision = input["expected_revision"]
                    .as_i64()
                    .ok_or(PlatformError::Validation("revision required"))?;
                let status = Policy::text(input, "status", 30)?;
                if !matches!(status, "new" | "in_progress" | "completed") {
                    return Err(PlatformError::Validation("invalid request status"));
                }
                let type_id = Policy::text(input, "type_id", 120)?;
                tx.query_opt(
                    "SELECT id FROM service_request_types WHERE id=$1 AND NOT archived FOR SHARE",
                    &[&type_id],
                )
                .map_err(Self::database_error)?
                .ok_or(PlatformError::Validation("request type unavailable"))?;
                let criticality = input
                    .get("criticality")
                    .map(|_| Policy::criticality(input))
                    .transpose()?;
                if input.get("product_id").is_some() {
                    let product = input["product_id"].as_str();
                    if !input["product_id"].is_null() && product.is_none() {
                        return Err(PlatformError::Validation("invalid product"));
                    }
                    if let Some(product) = product {
                        tx.query_opt(
                            "SELECT id FROM products WHERE id=$1 AND NOT archived FOR SHARE",
                            &[&product],
                        )
                        .map_err(Self::database_error)?
                        .ok_or(PlatformError::Validation("product unavailable"))?;
                    }
                    tx.execute(
                        "UPDATE service_requests SET product_id=$2 WHERE id=$1",
                        &[&resource, &product],
                    )
                    .map_err(Self::database_error)?;
                }
                if tx.execute("UPDATE service_requests SET status=$1,sla_snapshot=CASE WHEN type_id<>$2 OR ( $5::text IS NOT NULL AND criticality<>$5) THEN request_sla_snapshot($2,coalesce($5,criticality),created_at) ELSE sla_snapshot END,type_id=$2,criticality=coalesce($5,criticality),first_attention_at=CASE WHEN $1 IN ('in_progress','completed') THEN coalesce(first_attention_at,clock_timestamp()) ELSE first_attention_at END,resolved_at=CASE WHEN $1='completed' THEN coalesce(resolved_at,clock_timestamp()) ELSE NULL END,revision=revision+1,updated_at=clock_timestamp() WHERE id=$3 AND revision=$4",&[&status,&type_id,&resource,&revision,&criticality]).map_err(Self::database_error)?!=1{return Err(PlatformError::Conflict("request changed"))}
                json!({"status":status,"type_id":type_id,"criticality":criticality})
            }
            "comment" => {
                resource = id.ok_or(PlatformError::NotFound)?.to_owned();
                let body = Policy::text(input, "body", 32000)?;
                tx.execute("INSERT INTO service_request_comments(id,request_id,actor_id,body) VALUES($1,$2,$3,$4)",&[&SecretGenerator::id("src")?,&resource,&ctx.actor_id(),&body]).map_err(Self::database_error)?;
                tx.execute("UPDATE service_requests SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1",&[&resource]).map_err(Self::database_error)?;
                if Policy::internal(&role) {
                    tx.execute("UPDATE service_requests SET first_attention_at=coalesce(first_attention_at,clock_timestamp()) WHERE id=$1",&[&resource]).map_err(Self::database_error)?;
                }
                json!({})
            }
            _ => return Err(PlatformError::Validation("invalid request action")),
        };
        let event = match action {
            "create" => "created",
            "change" => "changed",
            _ => "commented",
        };
        tx.execute("INSERT INTO service_request_history(request_id,actor_id,action,detail) VALUES($1,$2,$3,$4)",&[&resource,&ctx.actor_id(),&event,&detail]).map_err(Self::database_error)?;
        let result: Value = tx
            .query_one(
                "SELECT to_jsonb(r) FROM service_requests r WHERE id=$1",
                &[&resource],
            )
            .map_err(Self::database_error)?
            .get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            &op,
            Some(&resource),
            detail,
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &result)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn save_service_request_type(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::platform_authority::PlatformAuthority::require(
            &mut tx,
            ctx,
            Capability::RequestsWrite,
            None,
        )?;
        if !matches!(
            Policy::role(&mut tx, ctx)?.as_str(),
            "manager" | "super_admin"
        ) {
            return Err(PlatformError::Forbidden);
        }
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "service_request.type",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let name = Policy::text(input, "name", 120)?;
        if let Some(sla) = input.get("sla") {
            Policy::validate_sla(sla)?;
        }
        let id = input["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or(SecretGenerator::id("srt")?);
        if input["id"].is_string() {
            let revision = input["expected_revision"]
                .as_i64()
                .ok_or(PlatformError::Validation("revision required"))?;
            let archived = input["archived"]
                .as_bool()
                .ok_or(PlatformError::Validation("archived required"))?;
            if tx.execute("UPDATE service_request_types SET name=$1,archived=$2,revision=revision+1 WHERE id=$3 AND revision=$4",&[&name,&archived,&id,&revision]).map_err(Self::database_error)?!=1{return Err(PlatformError::Conflict("request type changed"))}
        } else {
            tx.execute(
                "INSERT INTO service_request_types(id,name) VALUES($1,$2)",
                &[&id, &name],
            )
            .map_err(Self::database_error)?;
        }
        if let Some(sla) = input.get("sla") {
            tx.execute(
                "UPDATE service_request_types SET sla=$2 WHERE id=$1",
                &[&id, &sla],
            )
            .map_err(Self::database_error)?;
        }
        if let Some(icon) = input.get("icon") {
            let parsed: WorkflowIcon = serde_json::from_value(icon.clone())
                .map_err(|_| PlatformError::Validation("invalid request type icon"))?;
            tx.execute(
                "UPDATE service_request_types SET icon=$2 WHERE id=$1",
                &[&id, &parsed.as_str()],
            )
            .map_err(Self::database_error)?;
        }
        if let Some(color) = input.get("color") {
            let value = color
                .as_str()
                .ok_or(PlatformError::Validation("invalid request type color"))?;
            let color = crate::workflow_color::workflow_color(Some(value))?
                .ok_or(PlatformError::Validation("request type color required"))?;
            tx.execute(
                "UPDATE service_request_types SET color=$2 WHERE id=$1",
                &[&id, &color],
            )
            .map_err(Self::database_error)?;
        }
        let result: Value = tx
            .query_one(
                "SELECT to_jsonb(t) FROM service_request_types t WHERE id=$1",
                &[&id],
            )
            .map_err(Self::database_error)?
            .get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "service_request.type",
            Some(&id),
            result.clone(),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &result)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
}
