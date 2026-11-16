use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct ProjectStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> ProjectStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn create(
        &mut self,
        ctx: &AuthContext,
        name: &str,
        description: Option<&str>,
        color: Option<&HexColor>,
    ) -> Result<Project, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ProjectsWrite, None)?;
        let name = DomainText::name(name)?;
        let normalized = name.to_lowercase();
        let description = description
            .map(|v| DomainText::required(v, 4000))
            .transpose()?;
        let color = color.map(HexColor::as_str);
        let id = SecretGenerator::id("prj")?;
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
            "project.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }

        tx.execute(
            "INSERT INTO projects(id,name,normalized_name,description,color) VALUES($1,$2,$3,$4,$5)",
            &[&id, &name, &normalized, &description, &color],
        )
        .map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "project.created",
            Some(&id),
            json!({"color":color}),
        )?;
        let value: Value = tx
            .query_one("SELECT to_jsonb(p) FROM projects p WHERE id=$1", &[&id])
            .map_err(PlatformStore::database_error)?
            .get(0);
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn list(&mut self, ctx: &AuthContext) -> Result<Vec<Project>, PlatformError> {
        self.page(ctx, None, 100, false).map(|page| page.items)
    }
    pub fn page(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<ProjectPage, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        if !(1..=100).contains(&limit) {
            return Err(PlatformError::Validation(
                "page limit must be between 1 and 100",
            ));
        }
        if let Some(cursor) = after
            && (cursor.len() != 36
                || !cursor.starts_with("prj_")
                || !cursor[4..].bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(PlatformError::Validation("invalid project cursor"));
        }
        let actor_all = ctx
            .grants()
            .iter()
            .any(|g| g.capability == Capability::ProjectsRead && g.project_id.is_none());
        let key_all = ctx
            .scope()
            .is_none_or(|s| s.all_projects && s.capabilities.contains(&Capability::ProjectsRead));
        let all = actor_all && key_all;
        let ids: Vec<String> = if actor_all {
            ctx.scope()
                .map(|s| {
                    if s.capabilities.contains(&Capability::ProjectsRead) {
                        s.project_ids.clone()
                    } else {
                        vec![]
                    }
                })
                .unwrap_or_default()
        } else {
            ctx.grants()
                .iter()
                .filter(|g| g.capability == Capability::ProjectsRead)
                .filter_map(|g| g.project_id.clone())
                .filter(|id| ctx.require(Capability::ProjectsRead, Some(id)).is_ok())
                .collect()
        };
        let rows = self.store.client.query(
            "SELECT to_jsonb(p) FROM projects p WHERE ($3 OR NOT archived) AND ($1 OR id=ANY($2::text[])) AND ($4::text IS NULL OR id > $4) ORDER BY id LIMIT $5",
            &[&all, &ids, &include_archived, &after, &(i64::from(limit) + 1)],
        ).map_err(PlatformStore::database_error)?;
        let mut items: Vec<Project> = rows
            .iter()
            .map(|r| PlatformStore::decode(r.get(0)))
            .collect::<Result<_, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_cursor = if more {
            items.last().map(|p| p.id.clone())
        } else {
            None
        };
        Ok(ProjectPage { items, next_cursor })
    }
    pub fn change(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        change: &ProjectChange,
    ) -> Result<Project, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ProjectsWrite, Some(id))?;
        if change.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected_revision must be positive",
            ));
        }
        let name = DomainText::name(&change.name)?;
        let normalized = name.to_lowercase();
        let description = change
            .description
            .as_deref()
            .map(|v| DomainText::required(v, 4000))
            .transpose()?;
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
            "project.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let previous = tx
            .query_opt(
                "SELECT revision,archived,color FROM projects WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        if previous.get::<_, i64>(0) != change.expected_revision {
            return Err(PlatformError::Conflict("project revision changed"));
        }
        let previous_color: Option<String> = previous.get(2);
        let color = match &change.color {
            ColorChange::Unchanged => previous_color,
            ColorChange::Clear => None,
            ColorChange::Set(color) => Some(color.as_str().to_owned()),
        };
        let value: Value = tx.query_one(
            "UPDATE projects p SET name=$2,normalized_name=$3,description=$4,archived=$5,color=$6,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(p)",
            &[&id,&name,&normalized,&description,&change.archived,&color],
        ).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "project.changed",
            Some(id),
            json!({"previous_revision":change.expected_revision,"revision":value["revision"],"previous_archived":previous.get::<_,bool>(1),"archived":change.archived,"previous_color":previous.get::<_,Option<String>>(2),"color":color}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn get(&mut self, ctx: &AuthContext, id: &str) -> Result<Project, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ProjectsRead, Some(id))
            .map_err(|_| PlatformError::NotFound)?;
        let row = self
            .store
            .client
            .query_opt("SELECT to_jsonb(p) FROM projects p WHERE id=$1", &[&id])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        PlatformStore::decode(row.get(0))
    }
}
