use crate::{
    PlatformStore, audit_writer::AuditWriter, git_author_policy::GitAuthorPolicy,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn archive_git_author(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        archived: bool,
    ) -> Result<Value, PlatformError> {
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        GitAuthorPolicy::lock(&mut tx)?;
        let row=tx.query_opt("SELECT project_id,provider,host,author_email FROM git_commit_authorship WHERE id=$1",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        let project: String = row.get(0);
        let provider: String = row.get(1);
        let host: String = row.get(2);
        let email: Option<String> = row.get(3);
        GitAuthorPolicy::manager(&mut tx, ctx, Some(&project))?;
        let email = email.ok_or(PlatformError::Validation("author email unavailable"))?;
        let operation = if archived {
            "git.author.archive"
        } else {
            "git.author.restore"
        };
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        if archived {
            tx.execute("INSERT INTO git_author_archives(project_id,provider,host,email,archived_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",&[&project,&provider,&host,&email,&ctx.actor_id()]).map_err(Self::database_error)?;
        } else {
            tx.execute("DELETE FROM git_author_archives WHERE project_id=$1 AND provider=$2 AND host=$3 AND email=$4",&[&project,&provider,&host,&email]).map_err(Self::database_error)?;
        }
        let result = json!({"archived":archived});
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            Some(id),
            json!({"project_id":project,"provider":provider,"host":host,"email":email}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
}
