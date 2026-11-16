use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::{
    ActorKind, AuthContext, AuthMethod, Capability, NavigationOrderChange, PlatformError,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) struct NavigationOrderStore;
impl NavigationOrderStore {
    const PATHS: [&'static str; 18] = [
        "/settings",
        "/settings/members",
        "/settings/products",
        "/settings/projects",
        "/settings/task-types",
        "/settings/workflows",
        "/settings/request-types",
        "/settings/working-hours",
        "/settings/labor-calendar",
        "/settings/integrations",
        "/settings/accounts",
        "/settings/analysis-profiles",
        "/settings/openai",
        "/settings/email",
        "/settings/email-template",
        "/settings/notifications",
        "/settings/stickers",
        "/settings/mcp",
    ];
    pub fn execute(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        input: Option<&NavigationOrderChange>,
    ) -> Result<Value, PlatformError> {
        if ctx.kind() != ActorKind::Human || ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let can_manage =
            match PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None) {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(error) => return Err(error),
            };
        if let Some(input) = input {
            if !can_manage {
                return Err(PlatformError::Forbidden);
            }
            if input.expected_revision < 0
                || input.paths.len() > 64
                || input
                    .paths
                    .iter()
                    .any(|path| !Self::PATHS.contains(&path.as_str()))
                || input.paths.iter().collect::<BTreeSet<_>>().len() != input.paths.len()
            {
                return Err(PlatformError::Validation(
                    "invalid settings navigation order",
                ));
            }
            tx.query_one(
                "SELECT revision FROM installation_navigation WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(PlatformStore::database_error)?;
            if let Some(value) = ReceiptWriter::read(
                &mut tx,
                &store.namespace,
                receipt.as_ref(),
                ctx.actor_id(),
                "settings.navigation.change",
            )? {
                tx.commit().map_err(PlatformStore::database_error)?;
                return Ok(value);
            }
            let paths = json!(input.paths);
            let row=tx.query_opt("UPDATE installation_navigation SET paths=$1,revision=revision+1 WHERE singleton AND revision=$2 RETURNING revision",&[&paths,&input.expected_revision]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("settings navigation order changed"))?;
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "settings.navigation.changed",
                None,
                json!({"paths":paths,"revision":row.get::<_,i64>(0)}),
            )?;
        }
        let row = tx
            .query_one(
                "SELECT paths,revision FROM installation_navigation WHERE singleton",
                &[],
            )
            .map_err(PlatformStore::database_error)?;
        let mut paths: Vec<String> = PlatformStore::decode(row.get::<_, Value>(0))?;
        for path in Self::PATHS {
            if !paths.iter().any(|p| p == path) {
                paths.push(path.into());
            }
        }
        let result = json!({"paths":paths,"revision":row.get::<_,i64>(1),"can_manage":can_manage});
        if input.is_some() {
            ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
