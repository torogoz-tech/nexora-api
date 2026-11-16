use crate::{
    GitNetworkPolicy, PlatformStore, PullRequestStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    repository_job_source::RepositoryJobSource,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub struct PullRequestLinksStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> PullRequestLinksStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    fn authority(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<String, PlatformError> {
        PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, Some(project))?;
        PlatformAuthority::project(tx, project)?;
        let row=tx.query_opt("SELECT revision,remote_id FROM project_repositories WHERE id=$1 AND project_id=$2 AND NOT archived",&[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        RepositoryJobSource::lock(tx, ctx, project, repository, row.get(0))?;
        row.get::<_, Option<String>>(1)
            .ok_or(PlatformError::NotFound)
    }
    fn allowed(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        cap: Capability,
        project: &str,
    ) -> Result<bool, PlatformError> {
        match PlatformAuthority::require(tx, ctx, cap, Some(project)) {
            Ok(()) => Ok(true),
            Err(PlatformError::Forbidden) => Ok(false),
            Err(e) => Err(e),
        }
    }
    fn record(tx: &mut Transaction<'_>, id: &str) -> Result<Value, PlatformError> {
        let row=tx.query_opt("SELECT to_jsonb(l)||jsonb_build_object('target_id',COALESCE(l.task_id,l.document_id),'title',COALESCE(t.title,v.title),'code',d.code) FROM pull_request_links l LEFT JOIN tasks t ON t.id=l.task_id AND t.project_id=l.project_id LEFT JOIN project_documents d ON d.id=l.document_id AND d.project_id=l.project_id LEFT JOIN project_document_versions v ON v.id=l.version_id AND v.document_id=l.document_id AND v.project_id=l.project_id WHERE l.id=$1",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        Ok(row.get(0))
    }
    pub fn list(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        number: u64,
        after: Option<&str>,
        limit: u16,
    ) -> Result<Value, PlatformError> {
        if number == 0
            || number > i64::MAX as u64
            || !(1..=100).contains(&limit)
            || after.is_some_and(|v| v.len() > 128)
        {
            return Err(PlatformError::Validation("invalid pull request link page"));
        }
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let remote = Self::authority(&mut tx, ctx, project, repository)?;
        let tasks = Self::allowed(&mut tx, ctx, Capability::TasksRead, project)?;
        let docs = Self::allowed(&mut tx, ctx, Capability::DocumentsRead, project)?;
        let unrestricted =
            crate::task_read_authority::TaskReadAuthority::unrestricted(&mut tx, ctx, project)?;
        let visibility = crate::task_read_predicate::task_read_predicate("$9", "$10");
        let rows=tx.query(&format!("SELECT id FROM pull_request_links WHERE project_id=$1 AND repository_id=$2 AND remote_id=$3 AND pull_request_number=$4 AND removed_at IS NULL AND (($5 AND kind='task' AND EXISTS(SELECT 1 FROM tasks t WHERE t.id=task_id AND t.project_id=pull_request_links.project_id AND {visibility})) OR ($6 AND kind='requirement')) AND ($7::text IS NULL OR id>$7) ORDER BY id LIMIT $8"),&[&project,&repository,&remote,&(number as i64),&tasks,&docs,&after,&(i64::from(limit)+1),&ctx.actor_id(),&unrestricted]).map_err(PlatformStore::database_error)?;
        let more = rows.len() > usize::from(limit);
        let mut items = Vec::new();
        for row in rows.iter().take(usize::from(limit)) {
            let id: String = row.get(0);
            items.push(Self::record(&mut tx, &id)?);
        }
        let next = if more {
            items
                .last()
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(json!({"items":items,"next_cursor":next,"restricted":!tasks||!docs}))
    }
    pub fn create(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        number: u64,
        input: &PullRequestLinkCreate,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        input.validate()?;
        let verified_source = crate::pull_request_source::PullRequestSource::load(
            self.store, ctx, project, repository, policy,
        )?;
        PullRequestStore::new(self.store).query(
            ctx,
            project,
            repository,
            &PullRequestQuery {
                operation: PullRequestOperation::Detail,
                number: Some(number),
                state: "open".into(),
                page: 1,
                limit: 1,
            },
            policy,
        )?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let remote = Self::authority(&mut tx, ctx, project, repository)?;
        let current = tx
            .query_one(
                "SELECT revision FROM project_repositories WHERE id=$1 AND project_id=$2",
                &[&repository, &project],
            )
            .map_err(PlatformStore::database_error)?;
        if verified_source.repository.remote_id.as_deref() != Some(remote.as_str())
            || current.get::<_, i64>(0) != verified_source.repository.revision
        {
            return Err(PlatformError::Conflict(
                "repository changed while verifying PR link",
            ));
        }
        let (read, write) = if input.kind == "task" {
            (Capability::TasksRead, Capability::TasksWrite)
        } else {
            (Capability::DocumentsRead, Capability::DocumentsWrite)
        };
        PlatformAuthority::require(&mut tx, ctx, read, Some(project))?;
        PlatformAuthority::require(&mut tx, ctx, write, Some(project))?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "pull_request.link.create",
        )? {
            if value["remote_id"].as_str() != Some(remote.as_str()) {
                return Err(PlatformError::Conflict(
                    "link receipt belongs to an earlier repository binding",
                ));
            }
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        let row=if input.kind=="task"{tx.query_opt("SELECT revision FROM tasks WHERE id=$1 AND project_id=$2 AND NOT archived FOR SHARE",&[&input.target_id,&project])}else{tx.query_opt("SELECT revision FROM project_documents WHERE id=$1 AND project_id=$2 AND kind='requirement' AND NOT archived FOR SHARE",&[&input.target_id,&project])}.map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, i64>(0) != input.target_revision {
            return Err(PlatformError::Conflict("link target revision changed"));
        }
        if let Some(version) = &input.version_id {
            tx.query_opt("SELECT id FROM project_document_versions WHERE id=$1 AND document_id=$2 AND project_id=$3 FOR SHARE",&[&version,&input.target_id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        }
        let id = SecretGenerator::id("prl")?;
        let task = if input.kind == "task" {
            Some(input.target_id.as_str())
        } else {
            None
        };
        let document = if input.kind == "requirement" {
            Some(input.target_id.as_str())
        } else {
            None
        };
        tx.execute("INSERT INTO pull_request_links(id,project_id,repository_id,remote_id,pull_request_number,kind,task_id,document_id,version_id,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",&[&id,&project,&repository,&remote,&(number as i64),&input.kind,&task,&document,&input.version_id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "pull_request.link.created",
            Some(&id),
            json!({"project_id":project,"repository_id":repository,"number":number,"kind":input.kind,"target_id":input.target_id,"version_id":input.version_id,"reason":input.reason}),
        )?;
        let value = Self::record(&mut tx, &id)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    pub fn remove(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        number: u64,
        id: &str,
        input: &PullRequestLinkRemove,
    ) -> Result<Value, PlatformError> {
        DomainText::required(&input.reason, 4000)?;
        if number == 0 || number > i64::MAX as u64 || input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "invalid pull request link removal",
            ));
        }
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let remote = Self::authority(&mut tx, ctx, project, repository)?;
        let row=tx.query_opt("SELECT kind,revision,removed_at IS NOT NULL FROM pull_request_links WHERE id=$1 AND project_id=$2 AND repository_id=$3 AND remote_id=$4 AND pull_request_number=$5 FOR UPDATE",&[&id,&project,&repository,&remote,&(number as i64)]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let task = row.get::<_, String>(0) == "task";
        PlatformAuthority::require(
            &mut tx,
            ctx,
            if task {
                Capability::TasksRead
            } else {
                Capability::DocumentsRead
            },
            Some(project),
        )?;
        PlatformAuthority::require(
            &mut tx,
            ctx,
            if task {
                Capability::TasksWrite
            } else {
                Capability::DocumentsWrite
            },
            Some(project),
        )?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "pull_request.link.remove",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        if row.get::<_, i64>(1) != input.expected_revision || row.get::<_, bool>(2) {
            return Err(PlatformError::Conflict("pull request link changed"));
        }
        tx.execute("UPDATE pull_request_links SET removed_at=clock_timestamp(),revision=revision+1 WHERE id=$1",&[&id]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "pull_request.link.removed",
            Some(id),
            json!({"project_id":project,"repository_id":repository,"number":number,"reason":input.reason}),
        )?;
        let value = Self::record(&mut tx, id)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
}
