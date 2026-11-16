use crate::{
    GitNetworkPolicy, PlatformStore, PreparedCodeReview, PullRequestStore, SecretGenerator,
    platform_authority::PlatformAuthority, pull_request_source::PullRequestSource,
    repository_job_source::RepositoryJobSource,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
/// Durable, single-execution reviews of an authorized and immutable PR diff.
pub struct CodeReviewStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> CodeReviewStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    fn authority(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        write: bool,
    ) -> Result<(), PlatformError> {
        PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, Some(project))?;
        if write {
            if ctx.kind() != ActorKind::Human || ctx.method() != AuthMethod::Session {
                return Err(PlatformError::Forbidden);
            }
            PlatformAuthority::require(tx, ctx, Capability::KnowledgeWrite, Some(project))?;
        }
        Ok(())
    }
    fn public(row: &postgres::Row) -> Value {
        let mut value: Value = row.get(0);
        if value["status"] == "running" && row.get::<_, bool>(1) {
            value["status"] = json!("interrupted");
        }
        value
    }
    fn projection() -> &'static str {
        "(to_jsonb(v)-'source'-'input'-'request_id'-'expected_head_sha') || jsonb_build_object('head_sha',v.input#>>'{pull_request,head_sha}','base_sha',v.input#>>'{pull_request,base_sha}'),v.status='running' AND v.created_at<now()-interval '3 minutes'"
    }
    fn source_value(source: &PullRequestSource) -> Value {
        json!({"repository_revision":source.repository.revision,"generation":source.repository.source_generation,"connection":source.connection.id,"connection_revision":source.connection.revision,"credential_revision":source.credential_revision,"remote_id":source.repository.remote_id})
    }
    fn lock_source(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        expected: &Value,
        pin_generation: bool,
    ) -> Result<(), PlatformError> {
        let revision = expected["repository_revision"]
            .as_i64()
            .ok_or(PlatformError::Unavailable)?;
        let locked = RepositoryJobSource::lock(tx, ctx, project, repository, revision)?;
        let row = tx
            .query_one(
                "SELECT credential_revision FROM git_connections WHERE id=$1",
                &[&locked.connection],
            )
            .map_err(PlatformStore::database_error)?;
        // Pushes advance the content generation without changing consent or identity.
        // Finalization is pinned to the PR head/base instead of this repository-wide epoch.
        if (pin_generation && expected["generation"] != locked.generation)
            || expected["connection"] != locked.connection
            || expected["connection_revision"] != locked.connection_revision
            || expected["credential_revision"] != row.get::<_, i64>(0)
        {
            return Err(PlatformError::Conflict(
                "repository source changed during review",
            ));
        }
        Ok(())
    }
    fn remote(
        &mut self,
        ctx: &AuthContext,
        p: &str,
        r: &str,
        n: u64,
        operation: PullRequestOperation,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        PullRequestStore::new(self.store).query(
            ctx,
            p,
            r,
            &PullRequestQuery {
                operation,
                number: Some(n),
                state: "all".into(),
                page: 1,
                limit: 31,
            },
            policy,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        ctx: &AuthContext,
        p: &str,
        r: &str,
        n: u64,
        request_id: &str,
        head: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<PreparedCodeReview, PlatformError> {
        if n == 0
            || n > i64::MAX as u64
            || !(8..=128).contains(&request_id.len())
            || !request_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
            || !Self::sha(head)
        {
            return Err(PlatformError::Validation("invalid code review request"));
        }
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, p, true)?;
        let existing=tx.query_opt(&format!("SELECT {},v.project_id,v.repository_id,v.pr_number,v.expected_head_sha FROM pull_request_reviews v WHERE requested_by=$1 AND request_id=$2",Self::projection()),&[&ctx.actor_id(),&request_id]).map_err(PlatformStore::database_error)?;
        if let Some(row) = existing {
            if row.get::<_, String>(2) != p
                || row.get::<_, String>(3) != r
                || row.get::<_, i64>(4) != n as i64
                || row.get::<_, String>(5) != head
            {
                return Err(PlatformError::Conflict(
                    "code review request identifier reused",
                ));
            }
            tx.commit().map_err(PlatformStore::database_error)?;
            PullRequestSource::load(self.store, ctx, p, r, policy)?;
            return Ok(PreparedCodeReview {
                review: Self::public(&row),
                input: None,
            });
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        let source = PullRequestSource::load(self.store, ctx, p, r, policy)?;
        let before = self.remote(ctx, p, r, n, PullRequestOperation::Detail, policy)?;
        let files = self.remote(ctx, p, r, n, PullRequestOperation::Files, policy)?;
        let after = self.remote(ctx, p, r, n, PullRequestOperation::Detail, policy)?;
        if before["head_sha"] != head
            || after["head_sha"] != head
            || before["base_sha"] != after["base_sha"]
            || !after["base_sha"].as_str().is_some_and(Self::sha)
        {
            return Err(PlatformError::Conflict(
                "pull request changed before review",
            ));
        }
        let items = files["items"]
            .as_array()
            .ok_or(PlatformError::Unavailable)?;
        if items.is_empty()
            || items.len() > 30
            || !files["next_page"].is_null()
            || after["changed_files"]
                .as_u64()
                .is_some_and(|v| v != items.len() as u64)
            || items.iter().any(|f| {
                f["patch_truncated"] != false || f["patch"].as_str().is_none_or(str::is_empty)
            })
        {
            return Err(PlatformError::Validation(
                "pull request diff is incomplete or exceeds review limits",
            ));
        }
        if items
            .iter()
            .map(|f| f["patch"].as_str().map_or(0, str::len))
            .sum::<usize>()
            > 192 * 1024
        {
            return Err(PlatformError::Validation(
                "pull request diff exceeds review limits",
            ));
        }
        let current = PullRequestSource::load(self.store, ctx, p, r, policy)?;
        if !source.matches(&current) {
            return Err(PlatformError::Conflict(
                "repository source changed before review",
            ));
        }
        let identity = Self::source_value(&current);
        let input = json!({"pull_request":{"number":n,"title":after["title"],"head_sha":head,"base_sha":after["base_sha"],"web_url":after["web_url"]},"files":items});
        if serde_json::to_vec(&input)
            .map_err(|_| PlatformError::Unavailable)?
            .len()
            > 262144
        {
            return Err(PlatformError::Validation(
                "pull request diff exceeds review limits",
            ));
        }
        let id = SecretGenerator::id("prv")?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, p, true)?;
        Self::lock_source(&mut tx, ctx, p, r, &identity, true)?;
        let inserted=tx.execute("INSERT INTO pull_request_reviews(id,project_id,repository_id,pr_number,requested_by,request_id,expected_head_sha,source,input,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'running') ON CONFLICT(requested_by,request_id) DO NOTHING",&[&id,&p,&r,&(n as i64),&ctx.actor_id(),&request_id,&head,&identity,&input]).map_err(PlatformStore::database_error)?;
        let row=tx.query_one(&format!("SELECT {},v.project_id,v.repository_id,v.pr_number,v.expected_head_sha FROM pull_request_reviews v WHERE requested_by=$1 AND request_id=$2",Self::projection()),&[&ctx.actor_id(),&request_id]).map_err(PlatformStore::database_error)?;
        if row.get::<_, String>(2) != p
            || row.get::<_, String>(3) != r
            || row.get::<_, i64>(4) != n as i64
            || row.get::<_, String>(5) != head
        {
            return Err(PlatformError::Conflict(
                "code review request identifier reused",
            ));
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(PreparedCodeReview {
            review: Self::public(&row),
            input: (inserted == 1).then_some(input),
        })
    }
    fn sha(value: &str) -> bool {
        (40..=64).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit())
    }
    pub fn list(
        &mut self,
        ctx: &AuthContext,
        p: &str,
        r: &str,
        n: u64,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let source = PullRequestSource::load(self.store, ctx, p, r, policy)?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, p, false)?;
        Self::lock_source(&mut tx, ctx, p, r, &Self::source_value(&source), true)?;
        let rows=tx.query(&format!("SELECT {} FROM pull_request_reviews v WHERE project_id=$1 AND repository_id=$2 AND pr_number=$3 ORDER BY created_at DESC,id DESC LIMIT 20",Self::projection()),&[&p,&r,&(n as i64)]).map_err(PlatformStore::database_error)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(json!({"items":rows.iter().map(Self::public).collect::<Vec<_>>()}))
    }
    pub fn get(
        &mut self,
        ctx: &AuthContext,
        p: &str,
        r: &str,
        n: u64,
        id: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let source = PullRequestSource::load(self.store, ctx, p, r, policy)?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, p, false)?;
        Self::lock_source(&mut tx, ctx, p, r, &Self::source_value(&source), true)?;
        let row=tx.query_opt(&format!("SELECT {} FROM pull_request_reviews v WHERE id=$1 AND project_id=$2 AND repository_id=$3 AND pr_number=$4",Self::projection()),&[&id,&p,&r,&(n as i64)]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(Self::public(&row))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn finish(
        &mut self,
        ctx: &AuthContext,
        p: &str,
        r: &str,
        n: u64,
        id: &str,
        report: Option<Value>,
        model: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let remote = if report.is_some() {
            Some(self.remote(ctx, p, r, n, PullRequestOperation::Detail, policy)?)
        } else {
            None
        };
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, p, true)?;
        let row=tx.query_opt("SELECT source,input,status,requested_by FROM pull_request_reviews WHERE id=$1 AND project_id=$2 AND repository_id=$3 AND pr_number=$4 FOR UPDATE",&[&id,&p,&r,&(n as i64)]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, String>(3) != ctx.actor_id() {
            return Err(PlatformError::Forbidden);
        }
        if row.get::<_, String>(2) == "running" {
            let source: Value = row.get(0);
            let input: Value = row.get(1);
            Self::lock_source(&mut tx, ctx, p, r, &source, false)?;
            let valid = report
                .as_ref()
                .is_some_and(|v| PullRequestReviewReport::validate(v, &input).is_ok());
            let status = if report.is_none() || !valid {
                "failed"
            } else if remote.as_ref().is_some_and(|v| {
                v["head_sha"] != input["pull_request"]["head_sha"]
                    || v["base_sha"] != input["pull_request"]["base_sha"]
            }) {
                "stale"
            } else {
                "completed"
            };
            let stored = if valid { report } else { None };
            let model: String = model.chars().take(128).collect();
            tx.execute("UPDATE pull_request_reviews SET status=$2,report=$3,model=$4,finished_at=now() WHERE id=$1",&[&id,&status,&stored,&model]).map_err(PlatformStore::database_error)?;
        }
        let row = tx
            .query_one(
                &format!(
                    "SELECT {} FROM pull_request_reviews v WHERE id=$1",
                    Self::projection()
                ),
                &[&id],
            )
            .map_err(PlatformStore::database_error)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(Self::public(&row))
    }
}
