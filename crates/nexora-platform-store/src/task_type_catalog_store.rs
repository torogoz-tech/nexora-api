use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter, task_read_authority::TaskReadAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskTypeCatalogStore;
impl TaskTypeCatalogStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<CatalogTaskTypePage, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|s| s.is_empty() || s.len() > 64) {
            return Err(PlatformError::Validation("invalid task type catalog query"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        match PlatformAuthority::require(&mut tx, ctx, Capability::TaskTypesRead, None).or_else(
            |error| match error {
                PlatformError::Forbidden => {
                    PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)
                }
                error => Err(error),
            },
        ) {
            Ok(()) => {}
            Err(PlatformError::Forbidden) => {
                if TaskReadAuthority::scope(&mut tx, ctx)?.projects.is_empty() {
                    let projects = tx
                        .query(
                            "SELECT id FROM projects WHERE NOT archived ORDER BY id",
                            &[],
                        )
                        .map_err(PlatformStore::database_error)?;
                    let mut permitted = false;
                    for row in projects {
                        let project: String = row.get(0);
                        match PlatformAuthority::require(
                            &mut tx,
                            ctx,
                            Capability::ProjectsWrite,
                            Some(&project),
                        ) {
                            Ok(()) => {
                                permitted = true;
                                break;
                            }
                            Err(PlatformError::Forbidden) => {}
                            Err(e) => return Err(e),
                        }
                    }
                    if !permitted {
                        return Err(PlatformError::Forbidden);
                    }
                }
            }
            Err(e) => return Err(e),
        }
        let mut items=tx.query("SELECT to_jsonb(c) FROM task_type_catalog c WHERE ($1::text IS NULL OR code>$1) ORDER BY code LIMIT $2",&[&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?.into_iter().map(|r|PlatformStore::decode::<CatalogTaskType>(r.get(0))).collect::<Result<Vec<_>,_>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|t| t.code.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(CatalogTaskTypePage { items, next_after })
    }
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        input: &CatalogTaskTypeCreate,
    ) -> Result<CatalogTaskType, PlatformError> {
        let code = input.code.trim().to_ascii_lowercase();
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(PlatformError::Validation("invalid task type code"));
        }
        let name = DomainText::name(&input.name)?;
        let description = input
            .description
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(|v| DomainText::required(v, 2000))
            .transpose()?;
        let color = input.color.as_ref().map(HexColor::as_str);
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "task_type_catalog.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(v);
        }
        if tx
            .query_opt("SELECT code FROM task_type_catalog WHERE code=$1", &[&code])
            .map_err(PlatformStore::database_error)?
            .is_some()
        {
            return Err(PlatformError::Conflict("task type already exists"));
        }
        let value:Value=tx.query_one("INSERT INTO task_type_catalog AS c(code,name,description,color,icon,purpose) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(c)",&[&code,&name,&description,&color,&input.icon.as_str(),&input.purpose.as_str()]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_type_catalog.created",
            Some(&code),
            json!({"after":value}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        code: &str,
        input: &CatalogTaskTypeChange,
    ) -> Result<CatalogTaskType, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("expected revision required"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let name = input.name.as_deref().map(DomainText::name).transpose()?;
        let description = input
            .description
            .as_deref()
            .map(|v| {
                if v.trim().is_empty() {
                    Ok(None)
                } else {
                    DomainText::required(v, 2000).map(Some)
                }
            })
            .transpose()?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "task_type_catalog.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(v);
        }
        let before: CatalogTaskType = PlatformStore::decode(
            tx.query_opt(
                "SELECT to_jsonb(c) FROM task_type_catalog c WHERE code=$1 FOR UPDATE",
                &[&code],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0),
        )?;
        if before.revision != input.expected_revision {
            return Err(PlatformError::Conflict("catalog revision changed"));
        }
        let color = input.color.apply(before.color.clone());
        let description = description.unwrap_or(before.description.clone());
        let value:Value=tx.query_one("UPDATE task_type_catalog c SET name=COALESCE($2,name),description=$3,color=$4,archived=COALESCE($5,archived),icon=COALESCE($6,icon),revision=revision+1 WHERE code=$1 RETURNING to_jsonb(c)",&[&code,&name,&description,&color.as_ref().map(HexColor::as_str),&input.archived,&input.icon.map(TaskTypeIcon::as_str)]).map_err(PlatformStore::database_error)?.get(0);
        // Metadata is shared; each project's archived flag remains its own selection.
        tx.execute("UPDATE task_types SET name=$2,description=$3,color=$4,revision=revision+1 WHERE code=$1",&[&code,&name.unwrap_or(before.name.clone()),&description,&color.as_ref().map(HexColor::as_str)]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_type_catalog.changed",
            Some(code),
            json!({"before":before,"after":value,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
}
