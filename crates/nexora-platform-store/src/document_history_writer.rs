use crate::{PlatformStore, audit_writer::AuditWriter};
use nexora_platform::{AuthContext, PlatformError};
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct DocumentHistoryWriter;
impl DocumentHistoryWriter {
    #[allow(clippy::too_many_arguments)]
    pub fn append(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        version: &str,
        event: &str,
        reason: &str,
        details: Value,
    ) -> Result<(), PlatformError> {
        tx.execute("INSERT INTO document_history(project_id,document_id,version_id,event,actor_id,credential_id,reason,details) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[&project,&document,&version,&event,&ctx.actor_id(),&ctx.credential_id(),&reason,&details]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            event,
            Some(document),
            json!({"project_id":project,"version_id":version,"reason":reason,"details":details}),
        )
    }
}
