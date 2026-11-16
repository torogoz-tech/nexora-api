use crate::{
    LocalSnapshotArguments, LocalSourceRevocationArguments,
    local_operator_session::LocalOperatorSession,
};
use nexora_platform::{Capability, LocalSnapshotAdoption, LocalSourceChange, PlatformError};
use nexora_platform_store::{LocalSnapshotRequest, PlatformStore};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};

pub struct LocalSnapshotCommands;
impl LocalSnapshotCommands {
    pub fn adopt(
        config: &DatabaseConnections,
        schema: &str,
        args: LocalSnapshotArguments,
    ) -> Result<Value, PlatformError> {
        Self::publish(config, schema, args, false)
    }
    pub fn reauthorize(
        config: &DatabaseConnections,
        schema: &str,
        args: LocalSnapshotArguments,
    ) -> Result<Value, PlatformError> {
        Self::publish(config, schema, args, true)
    }
    fn publish(
        config: &DatabaseConnections,
        schema: &str,
        args: LocalSnapshotArguments,
        reauthorize: bool,
    ) -> Result<Value, PlatformError> {
        let mut store = Self::connect(config, schema)?;
        let session = LocalOperatorSession::open(&mut store, &args.operator)?;
        let ctx = session.context.clone();
        let result = (|| {
            ctx.require(Capability::InstallationManage, None)?;
            ctx.require(Capability::RepositoriesWrite, Some(&args.project))?;
            let input = LocalSnapshotAdoption {
                name: args.name,
                root: args.root,
                runtime_tenant_id: args.runtime_tenant,
                snapshot_id: args.snapshot,
                expected_payload_hash: args.expected_hash,
                reason: args.reason,
                repository_id: args.repository,
                expected_revision: args.expected_revision,
            };
            let canonical = LocalSnapshotRequest::canonical(&args.project, &input)?;
            let operation = if reauthorize {
                "local_source.reauthorize"
            } else {
                "local_snapshot.adopt"
            };
            let publication =
                store.request(&ctx, operation, &args.request_id, &canonical, |store| {
                    if reauthorize {
                        store.reauthorize_local_source(&ctx, &args.project, &input)
                    } else {
                        store.adopt_local_snapshot(&ctx, &args.project, &input)
                    }
                })?;
            serde_json::to_value(publication).map_err(|_| PlatformError::Storage)
        })();
        session.close(&mut store)?;
        result
    }
    pub fn revoke(
        config: &DatabaseConnections,
        schema: &str,
        args: LocalSourceRevocationArguments,
    ) -> Result<Value, PlatformError> {
        let mut store = Self::connect(config, schema)?;
        let session = LocalOperatorSession::open(&mut store, &args.operator)?;
        let ctx = session.context.clone();
        let input = LocalSourceChange {
            expected_revision: args.expected_revision,
            reason: args.reason,
        };
        let canonical =
            json!({"project_id":args.project,"repository_id":args.repository,"change":input});
        let result = store
            .request(
                &ctx,
                "local_source.revoke",
                &args.request_id,
                &canonical,
                |store| store.revoke_local_source(&ctx, &args.project, &args.repository, &input),
            )
            .and_then(|value| serde_json::to_value(value).map_err(|_| PlatformError::Storage));
        session.close(&mut store)?;
        result
    }
    fn connect(config: &DatabaseConnections, schema: &str) -> Result<PlatformStore, PlatformError> {
        let mut store = PlatformStore::connect(config, schema)?;
        if !store.ready()? {
            return Err(PlatformError::Conflict(
                "platform migrations must be applied before local source commands",
            ));
        }
        Ok(store)
    }
}
