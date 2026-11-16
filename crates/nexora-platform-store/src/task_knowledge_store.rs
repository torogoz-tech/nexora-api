use crate::{
    PlatformStore, platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn link_task_knowledge(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskKnowledgeLink,
    ) -> Result<Value, PlatformError> {
        if !matches!(input.kind.as_str(), "feature" | "requirement")
            || input.target_id.is_empty()
            || input.target_id.len() > 128
            || input.expected_revision < 1
            || input.expected_target_revision < 1
        {
            return Err(PlatformError::Validation("invalid task knowledge link"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = self.request_context.clone();
        let namespace = self.namespace.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.query_opt("SELECT id FROM tasks WHERE id=$1 FOR UPDATE", &[&id])
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksRead)?;
        let before = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksWrite)?;
        TaskScopeAuthority::modify(&mut tx, ctx, before.project_id.as_deref(), id, false)?;
        let project = before
            .project_id
            .as_deref()
            .ok_or(PlatformError::Validation(
                "task needs the same project as its knowledge links",
            ))?;
        PlatformAuthority::require(&mut tx, ctx, Capability::DocumentsRead, Some(project))?;
        if input.kind == "requirement" {
            let row=tx.query_opt("SELECT id FROM project_documents WHERE id=$1 AND project_id=$2 AND kind='requirement'",&[&input.target_id,&project]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
            let _: String = row.get(0);
            tx.commit().map_err(Self::database_error)?;
            let value = self.create_document_link(
                ctx,
                project,
                &input.target_id,
                &DocumentLinkCreate {
                    expected_document_revision: input.expected_target_revision,
                    version_id: input
                        .version_id
                        .clone()
                        .ok_or(PlatformError::Validation("requirement version required"))?,
                    target: DocumentLinkTarget::Task {
                        task_id: id.into(),
                        expected_revision: input.expected_revision,
                    },
                    kind: DocumentLinkKind::References,
                    reason,
                },
            )?;
            return serde_json::to_value(value).map_err(|_| PlatformError::Storage);
        }
        if input.version_id.is_some() {
            return Err(PlatformError::Validation(
                "feature does not accept a document version",
            ));
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.knowledge.link",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        if before.revision != input.expected_revision || before.archived {
            return Err(PlatformError::Conflict("task revision changed"));
        }
        tx.query_opt("SELECT id FROM project_features WHERE id=$1 AND project_id=$2 AND revision=$3 AND NOT archived FOR SHARE",&[&input.target_id,&project,&input.expected_target_revision]).map_err(Self::database_error)?.ok_or(PlatformError::Conflict("feature changed or unavailable"))?;
        let count:i64=tx.query_one("SELECT count(*) FROM task_feature_associations WHERE task_id=$1 AND feature_id<>$2",&[&id,&input.target_id]).map_err(Self::database_error)?.get(0);
        if count >= 32 {
            return Err(PlatformError::Validation("too many task feature links"));
        }
        tx.execute("INSERT INTO task_feature_associations(task_id,feature_id,linked_by) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&id,&input.target_id,&ctx.actor_id()]).map_err(Self::database_error)?;
        let value:Value=tx.query_one("UPDATE tasks t SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&id]).map_err(Self::database_error)?.get(0);
        let task: WorkTask = Self::decode(value.clone())?;
        TaskHistoryWriter::append(
            &mut tx,
            ctx,
            &task,
            "task.knowledge.linked",
            Some(&before),
            Some(&reason),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn task_knowledge_options(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        kind: &str,
        after: Option<&str>,
    ) -> Result<Value, PlatformError> {
        if !matches!(kind, "feature" | "requirement") || after.is_some_and(|a| a.len() > 128) {
            return Err(PlatformError::Validation("invalid task knowledge query"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksRead)?;
        let project = task
            .project_id
            .as_deref()
            .ok_or(PlatformError::Validation("task has no project"))?;
        PlatformAuthority::require(&mut tx, ctx, Capability::DocumentsRead, Some(project))?;
        let sql = if kind == "feature" {
            "SELECT jsonb_build_object('id',id,'name',name,'revision',revision,'version_id',NULL) FROM project_features WHERE project_id=$1 AND NOT archived AND ($2::text IS NULL OR id>$2) ORDER BY id LIMIT 21"
        } else {
            "SELECT jsonb_build_object('id',d.id,'name',v.title,'revision',d.revision,'version_id',v.id) FROM project_documents d JOIN project_document_versions v ON v.id=d.latest_version_id WHERE d.project_id=$1 AND d.kind='requirement' AND NOT d.archived AND ($2::text IS NULL OR d.id>$2) ORDER BY d.id LIMIT 21"
        };
        let mut rows: Vec<Value> = tx
            .query(sql, &[&project, &after])
            .map_err(Self::database_error)?
            .iter()
            .map(|r| r.get(0))
            .collect();
        let more = rows.len() > 20;
        rows.truncate(20);
        let next = if more {
            rows.last().map(|r| r["id"].clone()).unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":rows,"next_after":next}))
    }
}
