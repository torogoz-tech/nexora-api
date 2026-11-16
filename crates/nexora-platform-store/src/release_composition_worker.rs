use crate::{
    GitNetworkPolicy, PlatformStore, authentication_store::AuthenticationStore,
    task_release_store::TaskReleaseStore, task_transition_provider::TaskTransitionProvider,
    task_transition_provider_authority::TaskTransitionProviderAuthority,
};
use nexora_platform::*;
use nexora_source::{
    GitFetchPolicy, GitFetcher, GitObjectFormat, GitReleaseComposer, GitTransferLimits,
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
impl PlatformStore {
    pub fn compose_one_release(
        &mut self,
        policy: &GitNetworkPolicy,
        binary: &Path,
        root: &Path,
    ) -> Result<bool, PlatformError> {
        let row=self.client.query_opt("SELECT id,release_id FROM task_release_composition_jobs WHERE status='queued' OR (status='running' AND updated_at<clock_timestamp()-interval '2 minutes') ORDER BY created_at,id LIMIT 1",&[]).map_err(Self::database_error)?;
        let Some(row) = row else { return Ok(false) };
        let job: String = row.get(0);
        let release: String = row.get(1);
        self.compose_release_candidate(policy, binary, root, &job, &release)
    }
    fn compose_release_candidate(
        &mut self,
        policy: &GitNetworkPolicy,
        binary: &Path,
        root: &Path,
        job: &str,
        release: &str,
    ) -> Result<bool, PlatformError> {
        let lock = format!("release-compose:{release}");
        if !self
            .client
            .query_one(
                "SELECT pg_try_advisory_lock(hashtextextended($1,0))",
                &[&lock],
            )
            .map_err(Self::database_error)?
            .get::<_, bool>(0)
        {
            return Ok(false);
        }
        let result = self.compose_locked_release(policy, binary, root, job, release);
        let unlocked = self
            .client
            .query_one(
                "SELECT pg_advisory_unlock(hashtextextended($1,0))",
                &[&lock],
            )
            .map_err(Self::database_error);
        unlocked?;
        result
    }
    fn compose_locked_release(
        &mut self,
        policy: &GitNetworkPolicy,
        binary: &Path,
        root: &Path,
        job: &str,
        release: &str,
    ) -> Result<bool, PlatformError> {
        // The first candidate read is advisory. Another worker may finish while this caller is descheduled before acquiring the lock.
        let fresh=self.client.query_opt("SELECT project_id,release_revision,actor_id,credential_id,auth_method,record FROM task_release_composition_jobs WHERE id=$1 AND release_id=$2 AND (status='queued' OR (status='running' AND updated_at<clock_timestamp()-interval '2 minutes'))",&[&job,&release]).map_err(Self::database_error)?;
        let Some(fresh) = fresh else { return Ok(false) };
        let project: String = fresh.get(0);
        let revision: i64 = fresh.get(1);
        let actor: String = fresh.get(2);
        let credential: String = fresh.get(3);
        let method: String = fresh.get(4);
        let mut record: Value = fresh.get(5);
        let result = (|| {
            let ctx = AuthenticationStore::new(self).load(
                if method == "api_key" {
                    AuthMethod::ApiKey
                } else {
                    AuthMethod::Session
                },
                &credential,
            )?;
            if ctx.actor_id() != actor {
                return Err(PlatformError::Unauthorized);
            }
            TaskReleaseStore::authorize_publication(self, &ctx, &project)?;
            let release_view = self.task_release(&ctx, &project, release)?;
            if release_view["revision"].as_i64() != Some(revision)
                || release_view["status"] != "prepared"
            {
                return Err(PlatformError::Conflict(
                    "release changed before composition worker execution",
                ));
            }
            TaskReleaseStore::validate_pins(self, &project, release)?;
            let branch = release_view["branch_name"]
                .as_str()
                .ok_or(PlatformError::Storage)?
                .to_owned();
            self.client.execute("UPDATE task_release_composition_jobs SET status='running',updated_at=clock_timestamp() WHERE id=$1",&[&job]).map_err(Self::database_error)?;
            let limits = GitTransferLimits {
                timeout: Duration::from_secs(120),
                max_pack_bytes: 128 * 1024 * 1024,
                max_repository_bytes: 512 * 1024 * 1024,
                max_repository_entries: 100_000,
            };
            let fetcher = GitFetcher::new(binary.into(), limits.clone())
                .map_err(|_| PlatformError::Unavailable)?;
            let composer = GitReleaseComposer::new(binary.into(), limits)
                .map_err(|_| PlatformError::Unavailable)?;
            let repositories=self.client.query("SELECT repository_id,composition_status FROM task_release_repositories WHERE release_id=$1 ORDER BY repository_id",&[&release]).map_err(Self::database_error)?;
            if record.get("repositories").is_none() {
                record["repositories"] = json!({});
            }
            for repository in repositories {
                let repository: String = repository.get(0);
                TaskReleaseStore::authorize_publication(self, &ctx, &project)?;
                let pins=self.client.query("SELECT DISTINCT commit_sha,min(sequence) AS position FROM task_release_commit_pins WHERE release_id=$1 AND repository_id=$2 GROUP BY commit_sha ORDER BY position,commit_sha",&[&release,&repository]).map_err(Self::database_error)?;
                let commits: Vec<String> = pins.into_iter().map(|row| row.get(0)).collect();
                if commits.is_empty() || commits.len() > 100 {
                    return Err(PlatformError::Validation(
                        "release composition requires between 1 and 100 explicit commits per repository",
                    ));
                }
                let (source, credentials, main) = TaskTransitionProvider::release_transport(
                    self,
                    &ctx,
                    &project,
                    &repository,
                    policy,
                )?;
                let fact = {
                    let mut tx = self.client.transaction().map_err(Self::database_error)?;
                    let fact = TaskTransitionProviderAuthority::lock(
                        &mut tx,
                        &ctx,
                        &project,
                        &repository,
                    )?;
                    tx.commit().map_err(Self::database_error)?;
                    fact
                };
                let origin = if source.connection.provider == GitProvider::Github {
                    &policy.github_origin
                } else {
                    &policy.gitlab_origin
                };
                let fetch_policy = GitFetchPolicy::new(origin).map_err(|_| {
                    PlatformError::Validation("invalid provider Git transport origin")
                })?;
                let remote = source
                    .repository
                    .remote_full_name
                    .as_deref()
                    .ok_or(PlatformError::Storage)?;
                let saved = &record["repositories"][&repository];
                if saved["base_sha"].is_string() && saved["base_sha"] != main {
                    return Err(PlatformError::Conflict(
                        "main changed after release composition began; prepare a new release revision",
                    ));
                }
                let destination = root.join(format!("{job}-{repository}"));
                if destination.exists() {
                    std::fs::remove_dir_all(&destination).map_err(|_| PlatformError::Storage)?;
                }
                let fetched = fetcher
                    .fetch(
                        &fetch_policy,
                        remote,
                        &main,
                        if main.len() == 64 {
                            GitObjectFormat::Sha256
                        } else {
                            GitObjectFormat::Sha1
                        },
                        &credentials,
                        &destination,
                    )
                    .map_err(|_| {
                        PlatformError::Conflict(
                            "release source fetch failed; no branch was published",
                        )
                    })?;
                let compose = (|| {
                    for commit in &commits {
                        fetcher.fetch_into(&fetch_policy,remote,commit,if commit.len()==64{GitObjectFormat::Sha256}else{GitObjectFormat::Sha1},&credentials,fetched.bare()).map_err(|_|PlatformError::Conflict("selected task commit cannot be fetched from its authorized provider"))?;
                    }
                    let message = format!(
                        "Nexora release {release}\n\nSelected commits:\n{}\n",
                        commits.join("\n")
                    );
                    let head=composer.compose(fetched.bare(),&main,&commits,&message).map_err(|_|PlatformError::Conflict("selected commits conflict or depend on unselected work; resolve the task commits before composing this release"))?;
                    if record["repositories"][&repository]["head_sha"].is_string()
                        && record["repositories"][&repository]["head_sha"] != head
                    {
                        return Err(PlatformError::Conflict(
                            "release composition intent changed",
                        ));
                    }
                    record["repositories"][&repository] = json!({"base_sha":main,"head_sha":head,"commits":commits,"branch_name":branch,"provenance":"server_composed","status":"prepared_to_push"});
                    self.client.execute("UPDATE task_release_composition_jobs SET record=$2,updated_at=clock_timestamp() WHERE id=$1",&[&job,&record]).map_err(Self::database_error)?;
                    {
                        let mut tx = self.client.transaction().map_err(Self::database_error)?;
                        if TaskTransitionProviderAuthority::lock(
                            &mut tx,
                            &ctx,
                            &project,
                            &repository,
                        )? != fact
                        {
                            return Err(PlatformError::Conflict(
                                "repository authority changed before release push",
                            ));
                        }
                        tx.commit().map_err(Self::database_error)?;
                    }
                    TaskReleaseStore::authorize_publication(self, &ctx, &project)?;
                    TaskReleaseStore::validate_pins(self, &project, release)?;
                    let current_release = self.task_release(&ctx, &project, release)?;
                    if current_release["revision"].as_i64() != Some(revision) {
                        return Err(PlatformError::Conflict(
                            "release changed before branch creation",
                        ));
                    }
                    match TaskTransitionProvider::release_ref(
                        self,
                        &ctx,
                        &project,
                        &repository,
                        &branch,
                        policy,
                    )? {
                        Some(existing) if existing == head => {}
                        Some(_) => {
                            return Err(PlatformError::Conflict(
                                "release branch already exists with another commit; it will not be overwritten",
                            ));
                        }
                        None => composer
                            .push_new(
                                fetched.bare(),
                                &fetch_policy,
                                remote,
                                &credentials,
                                &head,
                                &branch,
                            )
                            .map_err(|_| PlatformError::Unavailable)?,
                    }
                    if TaskTransitionProvider::release_ref(
                        self,
                        &ctx,
                        &project,
                        &repository,
                        &branch,
                        policy,
                    )?
                    .as_deref()
                        != Some(head.as_str())
                    {
                        return Err(PlatformError::Unavailable);
                    }
                    record["repositories"][&repository]["status"] = json!("confirmed");
                    let mut tx = self.client.transaction().map_err(Self::database_error)?;
                    if TaskTransitionProviderAuthority::lock(&mut tx, &ctx, &project, &repository)?
                        != fact
                    {
                        return Err(PlatformError::Conflict(
                            "repository authority changed after release branch creation",
                        ));
                    }
                    tx.execute("UPDATE task_release_repositories SET head_sha=$3,composition_status='composed',composition_evidence=$4 WHERE release_id=$1 AND repository_id=$2",&[&release,&repository,&head,&record["repositories"][&repository]]).map_err(Self::database_error)?;
                    tx.execute("UPDATE task_release_composition_jobs SET record=$2,updated_at=clock_timestamp() WHERE id=$1",&[&job,&record]).map_err(Self::database_error)?;
                    tx.commit().map_err(Self::database_error)?;
                    Ok(())
                })();
                let _ = std::fs::remove_dir_all(&destination);
                compose?;
            }
            let mut tx = self.client.transaction().map_err(Self::database_error)?;
            let current = tx
                .query_one(
                    "SELECT revision FROM task_releases WHERE id=$1 FOR UPDATE",
                    &[&release],
                )
                .map_err(Self::database_error)?
                .get::<_, i64>(0);
            if current != revision {
                return Err(PlatformError::Conflict(
                    "release changed during composition",
                ));
            }
            tx.execute(
                "UPDATE task_releases SET revision=revision+1 WHERE id=$1",
                &[&release],
            )
            .map_err(Self::database_error)?;
            tx.execute("UPDATE task_release_composition_jobs SET status='succeeded',record=$2,updated_at=clock_timestamp() WHERE id=$1",&[&job,&record]).map_err(Self::database_error)?;
            crate::audit_writer::AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "release.composed",
                Some(release),
                json!({"job_id":job,"project_id":project,"evidence":record}),
            )?;
            tx.commit().map_err(Self::database_error)?;
            Ok(())
        })();
        if let Err(ref error) = result {
            record["error"] = json!(match error {
                PlatformError::Conflict(message) | PlatformError::Validation(message) => *message,
                _ =>
                    "release composition failed; inspect authority, provider and worker availability",
            });
            self.client.execute("UPDATE task_release_composition_jobs SET status='failed',record=$2,updated_at=clock_timestamp() WHERE id=$1",&[&job,&record]).map_err(Self::database_error)?;
        }
        result.map(|_| true).or_else(|error| {
            if matches!(error, PlatformError::Storage) {
                Err(error)
            } else {
                Ok(true)
            }
        })
    }
}

#[cfg(test)]
#[path = "release_composition_worker_tests.rs"]
mod tests;
