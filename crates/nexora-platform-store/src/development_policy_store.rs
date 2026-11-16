use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter, repository_access::RepositoryAccess,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::json;
pub(crate) struct DevelopmentPolicyStore;
impl DevelopmentPolicyStore {
    pub fn lock(
        tx: &mut Transaction<'_>,
        project: &str,
        repository: &str,
    ) -> Result<DevelopmentPolicy, PlatformError> {
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("development-policy:{project}:{repository}")],
        )
        .map_err(PlatformStore::database_error)?;
        let row=tx.query_opt("SELECT base_branch,branch_prefix,revision FROM repository_development_policies WHERE project_id=$1 AND repository_id=$2 FOR SHARE",&[&project,&repository]).map_err(PlatformStore::database_error)?;
        Ok(row
            .map(|r| DevelopmentPolicy {
                repository_id: repository.into(),
                base_branch: r.get(0),
                branch_prefix: r.get(1),
                revision: r.get(2),
            })
            .unwrap_or(DevelopmentPolicy {
                repository_id: repository.into(),
                base_branch: "develop".into(),
                branch_prefix: "feature".into(),
                revision: 0,
            }))
    }
}
impl PlatformStore {
    pub fn development_policy(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<DevelopmentPolicy, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        RepositoryAccess::lock(&mut tx, ctx, project, repository)?;
        let policy = DevelopmentPolicyStore::lock(&mut tx, project, repository)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(policy)
    }
    pub fn change_development_policy(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &DevelopmentPolicyChange,
    ) -> Result<DevelopmentPolicy, PlatformError> {
        if input.expected_revision < 0
            || !DevelopmentPolicy::valid_ref(&input.base_branch)
            || !DevelopmentPolicy::valid_ref(&input.branch_prefix)
            || input.branch_prefix.len() > 40
        {
            return Err(PlatformError::Validation("invalid development policy"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::RepositoriesWrite, Some(project))?;
        PlatformAuthority::require(&mut tx, ctx, Capability::WorkflowsWrite, Some(project))?;
        RepositoryAccess::lock(&mut tx, ctx, project, repository)?;
        let previous = DevelopmentPolicyStore::lock(&mut tx, project, repository)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "development_policy.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        if previous.revision != input.expected_revision {
            return Err(PlatformError::Conflict(
                "development policy revision changed",
            ));
        }
        let policy = DevelopmentPolicy {
            repository_id: repository.into(),
            base_branch: input.base_branch.clone(),
            branch_prefix: input.branch_prefix.clone(),
            revision: previous.revision + 1,
        };
        tx.execute("INSERT INTO repository_development_policies(repository_id,project_id,base_branch,branch_prefix,revision) VALUES($1,$2,$3,$4,$5) ON CONFLICT(repository_id) DO UPDATE SET base_branch=EXCLUDED.base_branch,branch_prefix=EXCLUDED.branch_prefix,revision=EXCLUDED.revision",&[&repository,&project,&policy.base_branch,&policy.branch_prefix,&policy.revision]).map_err(Self::database_error)?;
        let value = serde_json::to_value(&policy).map_err(|_| PlatformError::Storage)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "development_policy.changed",
            Some(repository),
            json!({"revision":policy.revision,"base_branch":policy.base_branch}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(policy)
    }
}
