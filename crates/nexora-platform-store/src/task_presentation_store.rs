use crate::{
    PlatformStore, platform_authority::PlatformAuthority, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    /// A task-read view. Business knowledge additionally requires its own current
    /// document-read authority; knowing a task ID never widens project access.
    pub fn pool_task_presentation(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksRead)?;
        let names=tx.query_one("SELECT (SELECT name FROM projects WHERE id=$1),(SELECT name FROM actors WHERE id=$2),(SELECT name FROM actors WHERE id=$3),COALESCE((SELECT name FROM task_type_catalog WHERE code=$4),(SELECT name FROM task_types WHERE project_id=$1 AND code=$4),$4)",&[&task.project_id,&task.assignee_id,&task.created_by,&task.type_code]).map_err(Self::database_error)?;
        let mut value = serde_json::to_value(&task).map_err(|_| PlatformError::Storage)?;
        value["project_name"] = json!(names.get::<_, Option<String>>(0));
        value["assignee_name"] = json!(names.get::<_, Option<String>>(1));
        value["created_by_name"] = json!(names.get::<_, Option<String>>(2));
        value["type_name"] = json!(names.get::<_, String>(3));
        let creator = tx.query_one("SELECT jsonb_build_object('id',a.id,'name',a.name,'position',COALESCE((SELECT position FROM member_positions WHERE actor_id=a.id),''),'avatar_revision',COALESCE((SELECT revision FROM user_avatars WHERE actor_id=a.id),0)) FROM actors a WHERE a.id=$1", &[&task.created_by]).map_err(Self::database_error)?;
        value["creator"] = creator.get(0);
        value["assignee"] = if task.assignee_id.as_deref() == Some(task.created_by.as_str()) {
            value["creator"].clone()
        } else if let Some(actor) = &task.assignee_id {
            tx.query_one("SELECT jsonb_build_object('id',a.id,'name',a.name,'position',COALESCE((SELECT position FROM member_positions WHERE actor_id=a.id),''),'avatar_revision',COALESCE((SELECT revision FROM user_avatars WHERE actor_id=a.id),0)) FROM actors a WHERE a.id=$1",&[&actor]).map_err(Self::database_error)?.get(0)
        } else {
            Value::Null
        };
        let mut knowledge = json!({"can_read":true,"feature":null,"documents":[],"has_more":false});
        if let Some(project) = task.project_id.as_deref() {
            match PlatformAuthority::require(&mut tx, ctx, Capability::DocumentsRead, Some(project))
            {
                Err(PlatformError::Forbidden) => knowledge["can_read"] = json!(false),
                Err(error) => return Err(error),
                Ok(()) => {
                    if let Some(row)=tx.query_opt("SELECT jsonb_build_object('id',f.id,'name',f.name,'archived',f.archived) FROM task_feature_links l JOIN project_features f ON f.id=l.feature_id AND f.project_id=l.project_id WHERE l.task_id=$1 AND l.project_id=$2",&[&id,&project]).map_err(Self::database_error)?{knowledge["feature"]=row.get(0);}
                    let rows=tx.query("SELECT to_jsonb(related) FROM (SELECT d.id,v.id AS version_id,v.title,d.kind,'requirement_origin'::text AS relation FROM task_feature_links l JOIN project_documents d ON d.id=l.requirement_id AND d.project_id=l.project_id JOIN project_document_versions v ON v.id=d.latest_version_id WHERE l.task_id=$1 AND l.project_id=$2 UNION SELECT d.id,v.id AS version_id,v.title,d.kind,l.kind AS relation FROM document_links l JOIN project_documents d ON d.id=l.source_document_id AND d.project_id=l.project_id JOIN project_document_versions v ON v.id=l.source_version_id AND v.document_id=d.id AND v.project_id=d.project_id WHERE l.target_task_id=$1 AND l.project_id=$2 AND l.removed_at IS NULL) related ORDER BY related.id,related.version_id,related.relation LIMIT 101",&[&id,&project]).map_err(Self::database_error)?;
                    knowledge["has_more"] = json!(rows.len() > 100);
                    knowledge["documents"] = json!(
                        rows.into_iter()
                            .take(100)
                            .map(|row| row.get::<_, Value>(0))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        let timing:Value=tx.query_one("WITH entries AS (SELECT created_at,details->'after'->>'lifecycle' AS lifecycle,lead(created_at,1,clock_timestamp()) OVER(ORDER BY sequence) AS until_at FROM task_history WHERE task_id=$1 AND jsonb_typeof(details->'after')='object') SELECT jsonb_build_object('active_seconds',COALESCE(sum(GREATEST(0,extract(epoch FROM until_at-created_at))) FILTER(WHERE lifecycle='active'),0)::bigint,'started_at',min(created_at) FILTER(WHERE lifecycle='active'),'completed_at',max(created_at) FILTER(WHERE lifecycle='completed')) FROM entries",&[&id]).map_err(Self::database_error)?.get(0);
        value["timing"] = timing;
        if knowledge["can_read"] == true && task.project_id.is_some() {
            knowledge["features"]=tx.query_one("SELECT COALESCE(jsonb_agg(jsonb_build_object('id',f.id,'name',f.name,'archived',f.archived) ORDER BY f.name,f.id),'[]'::jsonb) FROM project_features f WHERE f.project_id=$2 AND (EXISTS(SELECT 1 FROM task_feature_associations a WHERE a.task_id=$1 AND a.feature_id=f.id) OR EXISTS(SELECT 1 FROM task_feature_links l WHERE l.task_id=$1 AND l.feature_id=f.id))",&[&id,&task.project_id]).map_err(Self::database_error)?.get(0);
        }
        value["knowledge"] = knowledge;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
