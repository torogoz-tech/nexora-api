use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct RepositoryStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> RepositoryStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn register(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &RepositoryRegistration,
    ) -> Result<ProjectRepository, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        let locator = input.locator()?;
        let id = SecretGenerator::id("repo")?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "repository.register",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        tx.query_opt(
            "SELECT id FROM projects WHERE id=$1 AND NOT archived FOR SHARE",
            &[&project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let value:Value=tx.query_one("INSERT INTO project_repositories AS r(id,project_id,provider,host,requested_full_name) VALUES($1,$2,$3,$4,$5) RETURNING repository_metadata(r)",&[&id,&project,&input.provider.as_str(),&input.provider.default_host(),&locator]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "repository.registered",
            Some(&id),
            json!({"project_id":project,"provider":input.provider,"authorization_status":"awaiting_authorization"}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn get(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectRepository, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesRead, Some(project))
            .map_err(|_| PlatformError::NotFound)?;
        let row = self
            .store
            .client
            .query_opt(
                "SELECT repository_metadata(r) FROM project_repositories r WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        PlatformStore::decode(row.get(0))
    }
    pub fn page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<RepositoryPage, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesRead, Some(project))
            .map_err(|_| PlatformError::NotFound)?;
        if !(1..=100).contains(&limit) {
            return Err(PlatformError::Validation(
                "page limit must be between 1 and 100",
            ));
        }
        if let Some(cursor) = after
            && (cursor.len() != 37
                || !cursor.starts_with("repo_")
                || !cursor[5..].bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(PlatformError::Validation("invalid repository cursor"));
        }
        self.store
            .client
            .query_opt("SELECT id FROM projects WHERE id=$1", &[&project])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let rows=self.store.client.query("SELECT repository_metadata(r) FROM project_repositories r WHERE project_id=$1 AND ($2 OR NOT archived) AND ($3::text IS NULL OR id>$3) ORDER BY id LIMIT $4",&[&project,&include_archived,&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items: Vec<ProjectRepository> = rows
            .iter()
            .map(|r| PlatformStore::decode(r.get(0)))
            .collect::<Result<_, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_cursor = if more {
            items.last().map(|r| r.id.clone())
        } else {
            None
        };
        Ok(RepositoryPage { items, next_cursor })
    }
    pub fn change(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &RepositoryChange,
    ) -> Result<ProjectRepository, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected_revision must be positive",
            ));
        }
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "repository.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let revision:i64=tx.query_opt("SELECT revision FROM project_repositories WHERE id=$1 AND project_id=$2 FOR UPDATE",&[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict("repository revision changed"));
        }
        let value:Value=tx.query_one("UPDATE project_repositories AS r SET archived=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING repository_metadata(r)",&[&id,&input.archived]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "repository.changed",
            Some(id),
            json!({"project_id":project,"previous_revision":revision,"revision":value["revision"],"archived":input.archived}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
}
