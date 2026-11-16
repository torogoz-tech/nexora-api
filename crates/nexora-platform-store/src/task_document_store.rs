use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_read_authority::TaskReadAuthority,
    task_read_predicate::task_read_predicate, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};
pub(crate) struct TaskDocumentStore;
impl TaskDocumentStore {
    pub fn upload(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        task_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, PlatformError> {
        let name = name.trim();
        if name.is_empty()
            || name.len() > 240
            || name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
            || bytes.is_empty()
            || bytes.len() > 20971520
        {
            return Err(PlatformError::Validation("invalid task document"));
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        // Serialize against project reassignment so documents always inherit current task scope.
        tx.query_opt("SELECT id FROM tasks WHERE id=$1 FOR UPDATE", &[&task_id])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, task_id, Capability::TasksWrite)?;
        if task.archived {
            return Err(PlatformError::Conflict("task is archived"));
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.document.upload",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        let count: i64 = tx
            .query_one(
                "SELECT count(*) FROM task_documents WHERE task_id=$1",
                &[&task_id],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        if count >= 100 {
            return Err(PlatformError::Validation("task document limit reached"));
        }
        let id = SecretGenerator::id("tdoc")?;
        let sha = SecretGenerator::checksum(bytes);
        let value:Value=tx.query_one("INSERT INTO task_documents AS d(id,task_id,name,payload,sha256,uploaded_by) VALUES($1,$2,$3,$4,$5,$6) RETURNING jsonb_build_object('id',id,'task_id',task_id,'name',name,'sha256',sha256,'uploaded_by',uploaded_by,'created_at',created_at,'size_bytes',octet_length(payload))",&[&id,&task_id,&name,&bytes,&sha,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &task,
            "task.document_added",
            json!({"document":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        task: Option<&str>,
        project: Option<&str>,
        search: &str,
        after: Option<&str>,
    ) -> Result<Value, PlatformError> {
        if search.len() > 256 || after.is_some_and(|v| v.len() > 128) {
            return Err(PlatformError::Validation("invalid document query"));
        }
        let mut tx = store
            .client
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .start()
            .map_err(PlatformStore::database_error)?;
        if let Some(id) = task {
            TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksRead)?;
        }
        let scope = TaskReadAuthority::scope(&mut tx, ctx)?;
        let global = match TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksRead, None) {
            Ok(()) => true,
            Err(PlatformError::Forbidden) => false,
            Err(e) => return Err(e),
        };
        let visible = task_read_predicate("$1", "t.project_id=ANY($3)");
        let sql = format!(
            "SELECT jsonb_build_object('id',d.id,'task_id',d.task_id,'name',d.name,'sha256',d.sha256,'uploaded_by',d.uploaded_by,'created_at',d.created_at,'size_bytes',octet_length(d.payload),'project_id',t.project_id,'task_title',t.title,'uploader_name',a.name) FROM task_documents d JOIN tasks t ON t.id=d.task_id JOIN actors a ON a.id=d.uploaded_by WHERE ((t.project_id=ANY($2) AND {visible}) OR (t.project_id IS NULL AND {global})) AND ($4::text IS NULL OR t.id=$4) AND ($4::text IS NOT NULL OR t.project_id IS NOT DISTINCT FROM $5) AND strpos(lower(d.name),lower($6))>0 AND ($7::text IS NULL OR d.id>$7) ORDER BY d.id LIMIT 21"
        );
        let mut items: Vec<Value> = tx
            .query(
                &sql,
                &[
                    &ctx.actor_id(),
                    &scope.projects,
                    &scope.unrestricted,
                    &task,
                    &project,
                    &search,
                    &after,
                ],
            )
            .map_err(PlatformStore::database_error)?
            .into_iter()
            .map(|r| r.get(0))
            .collect();
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
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(json!({"items":items,"next_after":next}))
    }
    pub fn content(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(String, Vec<u8>), PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let row=tx.query_opt("SELECT d.task_id,d.name,d.payload,d.sha256 FROM task_documents d JOIN tasks t ON t.id=d.task_id WHERE d.id=$1 FOR SHARE OF t",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let task: String = row.get(0);
        TaskScopeAuthority::task(&mut tx, ctx, &task, Capability::TasksRead)?;
        let bytes: Vec<u8> = row.get(2);
        if SecretGenerator::checksum(&bytes) != row.get::<_, String>(3) {
            return Err(PlatformError::Storage);
        }
        let name = row.get(1);
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok((name, bytes))
    }
}
