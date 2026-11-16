use crate::{PlatformStore, service_request_policy::ServiceRequestPolicy as Policy};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn service_requests(
        &mut self,
        ctx: &AuthContext,
        search: &str,
        status: &str,
        page: i64,
    ) -> Result<Value, PlatformError> {
        self.service_requests_filtered(ctx, search, status, "", page)
    }
    pub fn service_requests_filtered(
        &mut self,
        ctx: &AuthContext,
        search: &str,
        status: &str,
        type_id: &str,
        page: i64,
    ) -> Result<Value, PlatformError> {
        self.service_requests_product_filtered(ctx, search, status, type_id, "", page)
    }
    pub fn service_requests_product_filtered(
        &mut self,
        ctx: &AuthContext,
        search: &str,
        status: &str,
        type_id: &str,
        product_id: &str,
        page: i64,
    ) -> Result<Value, PlatformError> {
        self.service_requests_product_filtered_with_criticality(
            ctx, search, status, type_id, product_id, "", page,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn service_requests_product_filtered_with_criticality(
        &mut self,
        ctx: &AuthContext,
        search: &str,
        status: &str,
        type_id: &str,
        product_id: &str,
        criticality: &str,
        page: i64,
    ) -> Result<Value, PlatformError> {
        if page < 1
            || search.len() > 300
            || type_id.len() > 120
            || product_id.len() > 120
            || criticality.len() > 20
            || !matches!(status, "" | "new" | "in_progress" | "completed")
            || !matches!(criticality, "" | "low" | "medium" | "high" | "critical")
        {
            return Err(PlatformError::Validation("invalid request filter"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let role = Policy::role(&mut tx, ctx)?;
        let all = role != "user";
        let offset = (page - 1)
            .checked_mul(20)
            .ok_or(PlatformError::Validation("invalid page"))?;
        let count:i64=tx.query_one("SELECT count(*) FROM service_requests r WHERE ($1 OR requester_id=$2) AND ($3='' OR status=$3) AND (strpos(lower(title),lower($4))>0 OR number::text=$4) AND ($5='' OR r.type_id=$5) AND ($6='' OR r.product_id=$6) AND ($7='' OR r.criticality=$7)",&[&all,&ctx.actor_id(),&status,&search,&type_id,&product_id,&criticality]).map_err(Self::database_error)?.get(0);
        let items:Vec<Value>=tx.query("SELECT to_jsonb(r)||jsonb_build_object('sla',request_sla_projection(r.sla_snapshot,r.first_attention_at,r.resolved_at),'requester_name',a.name,'type_name',t.name,'type_icon',t.icon,'type_color',t.color,'product_name',(SELECT name FROM products p WHERE p.id=r.product_id)) FROM service_requests r JOIN actors a ON a.id=r.requester_id JOIN service_request_types t ON t.id=r.type_id WHERE ($1 OR requester_id=$2) AND ($3='' OR status=$3) AND (strpos(lower(title),lower($4))>0 OR number::text=$4) AND ($5='' OR r.type_id=$5) AND ($6='' OR r.product_id=$6) AND ($7='' OR r.criticality=$7) ORDER BY r.created_at DESC,r.id LIMIT 20 OFFSET $8",&[&all,&ctx.actor_id(),&status,&search,&type_id,&product_id,&criticality,&offset]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items,"total":count,"page":page,"can_manage":Policy::internal(&role)}))
    }
    pub fn service_request(&mut self, ctx: &AuthContext, id: &str) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let role = Policy::visible(&mut tx, ctx, id, false)?;
        let mut item:Value=tx.query_one("SELECT to_jsonb(r)||jsonb_build_object('sla',request_sla_projection(r.sla_snapshot,r.first_attention_at,r.resolved_at),'requester_name',a.name,'type_name',t.name,'type_icon',t.icon,'type_color',t.color,'product_name',(SELECT name FROM products p WHERE p.id=r.product_id)) FROM service_requests r JOIN actors a ON a.id=r.requester_id JOIN service_request_types t ON t.id=r.type_id WHERE r.id=$1",&[&id]).map_err(Self::database_error)?.get(0);
        // Shared conversation never includes technical notes. Technical links are a separate projection.
        item["comments"]=json!(tx.query("SELECT jsonb_build_object('id',c.id,'body',c.body,'author_name',a.name,'created_at',c.created_at) FROM service_request_comments c JOIN actors a ON a.id=c.actor_id WHERE request_id=$1 ORDER BY c.created_at,c.id",&[&id]).map_err(Self::database_error)?.iter().map(|r|r.get::<_,Value>(0)).collect::<Vec<_>>());
        item["attachments"]=json!(tx.query("SELECT jsonb_build_object('id',id,'name',name,'media_type',media_type,'size',octet_length(content)) FROM service_request_attachments WHERE request_id=$1 ORDER BY created_at,id",&[&id]).map_err(Self::database_error)?.iter().map(|r|r.get::<_,Value>(0)).collect::<Vec<_>>());
        item["history"]=json!(tx.query("SELECT jsonb_build_object('action',h.action,'created_at',h.created_at,'author_name',a.name,'detail',h.detail) FROM service_request_history h JOIN actors a ON a.id=h.actor_id WHERE request_id=$1 AND action IN ('created','changed','commented','attached') ORDER BY h.id",&[&id]).map_err(Self::database_error)?.iter().map(|r|r.get::<_,Value>(0)).collect::<Vec<_>>());
        item["can_manage"] = json!(Policy::internal(&role));
        if Policy::internal(&role) {
            let links=tx.query("SELECT kind,target_id,project_id FROM service_request_links WHERE request_id=$1 ORDER BY created_at",&[&id]).map_err(Self::database_error)?;
            let mut visible = Vec::new();
            for row in links {
                let kind: String = row.get(0);
                let target: String = row.get(1);
                let project: Option<String> = row.get(2);
                let allowed = if kind == "task" {
                    crate::task_scope_authority::TaskScopeAuthority::task(
                        &mut tx,
                        ctx,
                        &target,
                        Capability::TasksRead,
                    )
                    .map(|_| ())
                } else {
                    crate::platform_authority::PlatformAuthority::require(
                        &mut tx,
                        ctx,
                        Capability::DocumentsRead,
                        project.as_deref(),
                    )
                };
                match allowed {
                    Ok(()) => {
                        visible.push(json!({"kind":kind,"target_id":target,"project_id":project}))
                    }
                    Err(PlatformError::Forbidden | PlatformError::NotFound) => {}
                    Err(e) => return Err(e),
                }
            }
            item["links"] = json!(visible);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(item)
    }
    pub fn service_request_types(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let role = Policy::role(&mut tx, ctx)?;
        let items = tx
            .query(
                "SELECT to_jsonb(t)||jsonb_build_object('total',(SELECT count(*) FROM service_requests r WHERE r.type_id=t.id AND ($1 OR r.requester_id=$2))) FROM service_request_types t ORDER BY lower(name),id",
                &[&(role != "user"), &ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .iter()
            .map(|r| r.get::<_, Value>(0))
            .collect::<Vec<_>>();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items,"can_manage":matches!(role.as_str(),"manager"|"super_admin")}))
    }
}
