use crate::{
    BranchProtectionApply, BranchProtectionPreview, GitNetworkPolicy, PlatformStore,
    audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    pull_request_client::PullRequestClient, receipt_writer::ReceiptWriter,
    repository_access::RepositoryAccess,
    repository_branch_protection_provider::RepositoryBranchProtectionProvider,
    task_transition_provider::TaskTransitionProvider,
    task_transition_provider_authority::TaskTransitionProviderAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub(crate) struct RepositoryBranchProtectionStore;
impl RepositoryBranchProtectionStore {
    fn manager(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        PlatformAuthority::require(tx, ctx, Capability::RepositoriesWrite, Some(project))?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        RepositoryAccess::lock(tx, ctx, project, repository)?;
        Ok(())
    }
    fn lock(tx: &mut Transaction<'_>, repository: &str) -> Result<(), PlatformError> {
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("branch-protection:{repository}")],
        )
        .map_err(PlatformStore::database_error)?;
        Ok(())
    }
    fn value(tx: &mut Transaction<'_>, repository: &str) -> Result<Value, PlatformError> {
        Ok(tx.query_opt("SELECT to_jsonb(p)-'authority'-'requested_by' FROM repository_branch_protection p WHERE repository_id=$1",&[&repository]).map_err(PlatformStore::database_error)?.map(|r|r.get(0)).unwrap_or(json!({"revision":0,"status":"unconfigured","required_checks":[],"plan":null,"result":null})))
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        RepositoryAccess::lock(&mut tx, ctx, project, repository)?;
        let value = Self::value(&mut tx, repository)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    pub fn preview(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &BranchProtectionPreview,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let checks: Vec<String> = input
            .required_checks
            .iter()
            .map(|s| s.trim().to_string())
            .collect();
        if checks.len() > 100
            || checks
                .iter()
                .any(|s| s.is_empty() || s.len() > 255 || s.chars().any(char::is_control))
            || checks
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != checks.len()
        {
            return Err(PlatformError::Validation("invalid required CI checks"));
        }
        let request = store.request_context.clone();
        {
            let mut tx = store
                .client
                .transaction()
                .map_err(PlatformStore::database_error)?;
            Self::manager(&mut tx, ctx, project, repository)?;
            if let Some(value) = ReceiptWriter::read(
                &mut tx,
                &store.namespace,
                request.as_ref(),
                ctx.actor_id(),
                "repository.branch_protection.preview",
            )? {
                tx.commit().map_err(PlatformStore::database_error)?;
                return Ok(value);
            }
            tx.commit().map_err(PlatformStore::database_error)?;
        }
        let (source, token) =
            TaskTransitionProvider::source(store, ctx, Some(project), repository, policy)?;
        let root = TaskTransitionProvider::root(&source)?;
        let authority = TaskTransitionProvider::authority(store, ctx, Some(project), repository)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let plan = RepositoryBranchProtectionProvider::preview(
            &client,
            source.connection.provider,
            &root,
            &token,
            &checks,
        )?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::manager(&mut tx, ctx, project, repository)?;
        Self::lock(&mut tx, repository)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "repository.branch_protection.preview",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        if TaskTransitionProviderAuthority::lock(&mut tx, ctx, project, repository)? != authority {
            return Err(PlatformError::Conflict(
                "repository authority changed; prepare branch protection again",
            ));
        }
        tx.execute("INSERT INTO repository_branch_protection(repository_id,project_id,revision,status,required_checks,authority,plan,requested_by) VALUES($1,$2,1,'prepared',$3,$4,$5,$6) ON CONFLICT(repository_id) DO UPDATE SET revision=repository_branch_protection.revision+1,status='prepared',required_checks=EXCLUDED.required_checks,authority=EXCLUDED.authority,plan=EXCLUDED.plan,result=NULL,requested_by=EXCLUDED.requested_by,updated_at=clock_timestamp()",&[&repository,&project,&json!(checks),&authority,&plan,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "repository.branch_protection.prepared",
            Some(repository),
            json!({"branches":["main","develop","stage","preprod"]}),
        )?;
        let value = Self::value(&mut tx, repository)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    pub fn apply(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &BranchProtectionApply,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "invalid branch protection revision",
            ));
        }
        let request = store.request_context.clone();
        // Persist intent before network writes. Replays reconcile the same policy after a lost response.
        let (checks, authority) = {
            let mut tx = store
                .client
                .transaction()
                .map_err(PlatformStore::database_error)?;
            Self::manager(&mut tx, ctx, project, repository)?;
            Self::lock(&mut tx, repository)?;
            if let Some(value) = ReceiptWriter::read(
                &mut tx,
                &store.namespace,
                request.as_ref(),
                ctx.actor_id(),
                "repository.branch_protection.apply",
            )? {
                tx.commit().map_err(PlatformStore::database_error)?;
                return Ok(value);
            }
            let row=tx.query_opt("SELECT revision,required_checks,authority FROM repository_branch_protection WHERE repository_id=$1 FOR UPDATE",&[&repository]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
            if row.get::<_, i64>(0) != input.expected_revision {
                return Err(PlatformError::Conflict("branch protection preview changed"));
            }
            let checks: Vec<String> = PlatformStore::decode(row.get(1))?;
            let authority: Value = row.get(2);
            if TaskTransitionProviderAuthority::lock(&mut tx, ctx, project, repository)?
                != authority
            {
                return Err(PlatformError::Conflict(
                    "repository authority changed; prepare branch protection again",
                ));
            }
            tx.execute("UPDATE repository_branch_protection SET status='applying',updated_at=clock_timestamp() WHERE repository_id=$1",&[&repository]).map_err(PlatformStore::database_error)?;
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "repository.branch_protection.requested",
                Some(repository),
                json!({"revision":input.expected_revision}),
            )?;
            tx.commit().map_err(PlatformStore::database_error)?;
            (checks, authority)
        };
        let (source, token) =
            TaskTransitionProvider::source(store, ctx, Some(project), repository, policy)?;
        let root = TaskTransitionProvider::root(&source)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::lock(&mut tx, repository)?;
        Self::manager(&mut tx, ctx, project, repository)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "repository.branch_protection.apply",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        let revision:i64=tx.query_one("SELECT revision FROM repository_branch_protection WHERE repository_id=$1 FOR UPDATE",&[&repository]).map_err(PlatformStore::database_error)?.get(0);
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict("branch protection preview changed"));
        }
        // Keep role, grants, credential and source locks throughout remote writes so revocation cannot race administration.
        if TaskTransitionProviderAuthority::lock(&mut tx, ctx, project, repository)? != authority {
            return Err(PlatformError::Conflict(
                "repository authority changed; prepare branch protection again",
            ));
        }
        let outcome = RepositoryBranchProtectionProvider::apply(
            &client,
            source.connection.provider,
            &root,
            &token,
            &checks,
        );
        let (status, result) = match &outcome {
            Ok(result) if result["verified"] == true => ("applied", result.clone()),
            Ok(result) => ("failed", result.clone()),
            Err(_) => (
                "failed",
                json!({"verified":false,"message":"The provider policy could not be verified. Refresh and retry the same prepared policy; inspect provider permissions, branch existence and required checks."}),
            ),
        };
        tx.execute("UPDATE repository_branch_protection SET status=$2,result=$3,updated_at=clock_timestamp() WHERE repository_id=$1",&[&repository,&status,&result]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "repository.branch_protection.finished",
            Some(repository),
            json!({"revision":revision,"status":status}),
        )?;
        let value = Self::value(&mut tx, repository)?;
        if status == "applied" {
            ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        outcome?;
        Ok(value)
    }
}
impl PlatformStore {
    pub fn repository_branch_protection(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<Value, PlatformError> {
        RepositoryBranchProtectionStore::get(self, ctx, project, repository)
    }
    pub fn preview_repository_branch_protection(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &BranchProtectionPreview,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        RepositoryBranchProtectionStore::preview(self, ctx, project, repository, input, policy)
    }
    pub fn apply_repository_branch_protection(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &BranchProtectionApply,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        RepositoryBranchProtectionStore::apply(self, ctx, project, repository, input, policy)
    }
}
