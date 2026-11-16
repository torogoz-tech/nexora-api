use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

impl PlatformStore {
    pub fn products(&mut self, ctx: &AuthContext, technical: bool) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        if !technical {
            PlatformAuthority::require(&mut tx, ctx, Capability::RequestsRead, None)?;
        } else {
            let candidates:Vec<String>=tx.query("SELECT p.id FROM projects p WHERE NOT p.archived AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='projects:read' AND (g.project_id IS NULL OR g.project_id=p.id))",&[&ctx.actor_id()]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
            let mut readable =
                PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsRead, None).is_ok();
            for p in candidates {
                if PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsRead, Some(&p))
                    .is_ok()
                {
                    readable = true;
                }
            }
            if !readable {
                return Err(PlatformError::Forbidden);
            }
        }
        let can_manage=tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(Self::database_error)?.is_some();
        let rows=tx.query("SELECT to_jsonb(p) FROM products p WHERE $1 OR NOT archived ORDER BY lower(name),id",&[&(technical&&can_manage)]).map_err(Self::database_error)?;
        let mut items = Vec::new();
        for row in rows {
            let mut item: Value = row.get(0);
            if technical {
                let links:Vec<String>=tx.query("SELECT pp.project_id FROM product_projects pp JOIN projects p ON p.id=pp.project_id WHERE pp.product_id=$1 AND NOT p.archived AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$2 AND g.capability='projects:read' AND (g.project_id IS NULL OR g.project_id=p.id)) ORDER BY p.name,p.id",&[&item["id"].as_str(),&ctx.actor_id()]).map_err(Self::database_error)?.into_iter().map(|r|r.get(0)).collect();
                // Enforce credential attenuation as well as the member's live grants.
                let mut allowed = Vec::new();
                for project in links {
                    match PlatformAuthority::require(
                        &mut tx,
                        ctx,
                        Capability::ProjectsRead,
                        Some(&project),
                    ) {
                        Ok(()) => allowed.push(project),
                        Err(PlatformError::Forbidden) => {}
                        Err(e) => return Err(e),
                    }
                }
                if allowed.is_empty() && !can_manage {
                    continue;
                }
                item["project_ids"] = json!(allowed);
            } else {
                item.as_object_mut().unwrap().remove("created_by");
            }
            items.push(item);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(
            json!({"items":items,"can_manage":technical&&can_manage&&ctx.method()==AuthMethod::Session}),
        )
    }
    pub fn save_product(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Forbidden)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "product.save",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let name = DomainText::required(
            input["name"]
                .as_str()
                .ok_or(PlatformError::Validation("product name required"))?,
            120,
        )?;
        let description = DomainText::optional(input["description"].as_str().unwrap_or(""), 4000)?;
        let projects: Vec<String> =
            Self::decode(input.get("project_ids").cloned().unwrap_or(json!([])))?;
        let unique: std::collections::BTreeSet<_> = projects.iter().collect();
        if unique.len() != projects.len() {
            return Err(PlatformError::Validation("duplicate product project"));
        }
        for project in &projects {
            PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsWrite, Some(project))?;
            PlatformAuthority::project(&mut tx, project)?;
        }
        let id = input["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or(SecretGenerator::id("prd")?);
        if input["id"].is_string() {
            let rev = input["expected_revision"]
                .as_i64()
                .ok_or(PlatformError::Validation("product revision required"))?;
            let archived = input["archived"]
                .as_bool()
                .ok_or(PlatformError::Validation("product archive state required"))?;
            if tx.execute("UPDATE products SET name=$2,description=$3,archived=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND revision=$5",&[&id,&name,&description,&archived,&rev]).map_err(Self::database_error)?!=1{return Err(PlatformError::Conflict("product changed"))}
        } else {
            tx.execute(
                "INSERT INTO products(id,name,description,created_by) VALUES($1,$2,$3,$4)",
                &[&id, &name, &description, &ctx.actor_id()],
            )
            .map_err(Self::database_error)?;
        }
        tx.execute("DELETE FROM product_projects WHERE product_id=$1", &[&id])
            .map_err(Self::database_error)?;
        for project in projects {
            tx.execute(
                "INSERT INTO product_projects(product_id,project_id) VALUES($1,$2)",
                &[&id, &project],
            )
            .map_err(Self::database_error)?;
        }
        let value: Value = tx
            .query_one("SELECT to_jsonb(p) FROM products p WHERE id=$1", &[&id])
            .map_err(Self::database_error)?
            .get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "product.save",
            Some(&id),
            value.clone(),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
