use crate::{
    PlatformStore, SecretGenerator, git_author_identity::GitAuthorIdentity,
    git_author_policy::GitAuthorPolicy, git_commit_identity::GitCommitIdentity,
};
use nexora_platform::PlatformError;
use postgres::Transaction;
use serde_json::Value;
pub(crate) struct GitAuthorObservationWriter;
impl GitAuthorObservationWriter {
    pub fn record(
        tx: &mut Transaction<'_>,
        provider: &str,
        host: &str,
        scope: &str,
        delivery: &str,
        repository: &str,
        payload: &Value,
    ) -> Result<(), PlatformError> {
        if payload["before"]
            .as_str()
            .is_some_and(|v| Some(v) == payload["after"].as_str())
        {
            return Ok(());
        }
        if payload["deleted"].as_bool() == Some(true)
            || payload["after"]
                .as_str()
                .is_none_or(|s| s.chars().all(|c| c == '0'))
        {
            return Ok(());
        }
        let Some(commits) = payload["commits"].as_array() else {
            return Ok(());
        };
        GitAuthorPolicy::lock(tx)?;
        let row=tx.query_opt("SELECT project_id,remote_id FROM project_repositories WHERE id=$1 AND authorization_status='authorized' AND NOT archived",&[&repository]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let project: String = row.get(0);
        let remote: String = row.get(1);
        for commit in commits
            .iter()
            .take(if provider == "github" { 2048 } else { 20 })
        {
            let Some(sha) = commit["id"]
                .as_str()
                .map(str::to_ascii_lowercase)
                .filter(|s| GitCommitIdentity::valid(s))
            else {
                continue;
            };
            let name = commit["author"]["name"]
                .as_str()
                .unwrap_or("")
                .chars()
                .filter(|c| *c != '\0')
                .take(256)
                .collect::<String>();
            let original = commit["author"]["email"].as_str();
            let email = original.and_then(GitAuthorIdentity::email);
            let raw = original.map(|s| {
                s.chars()
                    .filter(|c| *c != '\0')
                    .take(320)
                    .collect::<String>()
            });
            let message = commit["message"].as_str().map(|value| {
                value
                    .chars()
                    .filter(|c| *c != '\0')
                    .take(16000)
                    .collect::<String>()
            });
            let id = SecretGenerator::id("gco")?;
            tx.execute("INSERT INTO git_commit_authorship(id,provider,host,remote_repository_id,repository_id,project_id,commit_sha,author_name,author_email,author_email_raw,scope_id,delivery_id,occurred_at,commit_message) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,COALESCE((SELECT min(occurred_at) FROM git_commit_authorship WHERE provider=$2 AND host=$3 AND remote_repository_id=$4 AND commit_sha=$7),clock_timestamp()),$13) ON CONFLICT(provider,host,remote_repository_id,commit_sha,project_id) DO UPDATE SET commit_message=COALESCE(EXCLUDED.commit_message,git_commit_authorship.commit_message)",&[&id,&provider,&host,&remote,&repository,&project,&sha,&name,&email,&raw,&scope,&delivery,&message]).map_err(PlatformStore::database_error)?;
        }
        if let Some(branch) = payload["ref"]
            .as_str()
            .and_then(|reference| reference.strip_prefix("refs/heads/"))
            .filter(|branch| {
                !branch.is_empty() && branch.len() <= 1024 && !branch.chars().any(char::is_control)
            })
        {
            for commit in commits
                .iter()
                .take(if provider == "github" { 2048 } else { 20 })
            {
                if let Some(sha) = commit["id"]
                    .as_str()
                    .map(str::to_ascii_lowercase)
                    .filter(|sha| GitCommitIdentity::valid(sha))
                {
                    tx.execute("INSERT INTO git_commit_branches(repository_id,commit_sha,branch_name) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&repository,&sha,&branch]).map_err(PlatformStore::database_error)?;
                }
            }
        }
        Self::attribute(tx, None, Some(repository))?;
        Ok(())
    }
    pub fn attribute(
        tx: &mut Transaction<'_>,
        alias: Option<&str>,
        repository: Option<&str>,
    ) -> Result<(), PlatformError> {
        let members=tx.query("SELECT member.id FROM actors member WHERE EXISTS(SELECT 1 FROM git_author_aliases a JOIN git_commit_authorship c ON c.author_email=a.email AND c.provider=a.provider AND c.host=a.host WHERE a.actor_id=member.id AND a.revoked_at IS NULL AND c.actor_id IS NULL AND ($1::text IS NULL OR a.id=$1) AND ($2::text IS NULL OR c.repository_id=$2)) ORDER BY member.id FOR SHARE OF member",&[&alias,&repository]).map_err(PlatformStore::database_error)?;
        for member in members {
            let id: String = member.get(0);
            tx.query("SELECT capability FROM actor_grants WHERE actor_id=$1 AND capability='repositories:read' FOR SHARE",&[&id]).map_err(PlatformStore::database_error)?;
        }
        // Authorship is organization-wide; query authorization separately controls who can read the work.
        let rows=tx.query("UPDATE git_commit_authorship c SET actor_id=a.actor_id,alias_id=a.id,attributed_at=clock_timestamp() FROM git_author_aliases a,actors member WHERE c.actor_id IS NULL AND c.author_email=a.email AND c.provider=a.provider AND c.host=a.host AND a.revoked_at IS NULL AND ($1::text IS NULL OR a.id=$1) AND ($2::text IS NULL OR c.repository_id=$2) AND member.id=a.actor_id AND member.active AND member.kind='human' RETURNING c.actor_id",&[&alias,&repository]).map_err(PlatformStore::database_error)?;
        for row in rows {
            let actor: String = row.get(0);
            tx.execute(
                "INSERT INTO developer_rating_dirty(actor_id) VALUES($1) ON CONFLICT DO NOTHING",
                &[&actor],
            )
            .map_err(PlatformStore::database_error)?;
        }
        Ok(())
    }
}
