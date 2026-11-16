use crate::{
    PlatformStore, RepositoryJobLease, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    repository_job_execution_authority::RepositoryJobExecutionAuthority,
};
use nexora_indexer::AnalysisProfileCompiler;
use nexora_language::AnalysisProfilePackage;
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub(crate) struct AnalysisProfileStore;
impl AnalysisProfileStore {
    fn admin(tx: &mut Transaction<'_>, ctx: &AuthContext) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        PlatformAuthority::require(tx, ctx, Capability::InstallationManage, None)
    }
    fn summary(tx: &mut Transaction<'_>, id: &str) -> Result<Value, PlatformError> {
        Ok(tx.query_opt("SELECT to_jsonb(p)-'package'-'created_by' FROM analysis_profile_versions p WHERE id=$1",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0))
    }
    pub fn import(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        input: &AnalysisProfilePackage,
    ) -> Result<Value, PlatformError> {
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        match PlatformAuthority::require(&mut tx, ctx, Capability::AnalysisProfilesContribute, None)
        {
            Ok(()) => {}
            Err(PlatformError::Forbidden) => Self::admin(&mut tx, ctx)?,
            Err(error) => return Err(error),
        }
        if serde_json::to_vec(input)
            .map_err(|_| PlatformError::Storage)?
            .len()
            > AnalysisProfilePackage::MAX_BYTES
        {
            return Err(PlatformError::Validation(
                "profile exceeds maximum package size",
            ));
        }
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "analysis_profile.import",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(v);
        }
        AnalysisProfileCompiler::compile(input).map_err(|_| {
            PlatformError::Validation("invalid or incompatible analysis profile models")
        })?;
        let checksum = input
            .checksum()
            .map_err(|_| PlatformError::Validation("invalid profile package"))?;
        tx.query_one("SELECT pg_advisory_xact_lock(hashtextextended(current_schema()||':analysis-profile-import',0))",&[]).map_err(PlatformStore::database_error)?;
        let id = if let Some(row) = tx
            .query_opt(
                "SELECT id,checksum FROM analysis_profile_versions WHERE key=$1 AND version=$2",
                &[&input.key, &input.version],
            )
            .map_err(PlatformStore::database_error)?
        {
            if row.get::<_, String>(1) != checksum {
                return Err(PlatformError::Conflict(
                    "profile key/version already has different content",
                ));
            }
            row.get::<_, String>(0)
        } else {
            let id = SecretGenerator::id("apv")?;
            let package =
                serde_json::to_value(input.canonical()).map_err(|_| PlatformError::Storage)?;
            tx.execute("INSERT INTO analysis_profile_versions(id,key,version,name,language_id,checksum,package,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[&id,&input.key,&input.version,&input.name,&input.language_id,&checksum,&package,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "analysis_profile.imported",
                Some(&id),
                json!({"key":input.key,"version":input.version,"checksum":checksum}),
            )?;
            id
        };
        let result = Self::summary(&mut tx, &id)?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn list(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<Value, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|v| v.is_empty() || v.len() > 128) {
            return Err(PlatformError::Validation("invalid analysis profile page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        match PlatformAuthority::require(&mut tx, ctx, Capability::AnalysisProfilesRead, None) {
            Ok(()) => {}
            Err(PlatformError::Forbidden) => Self::admin(&mut tx, ctx)?,
            Err(error) => return Err(error),
        }
        let rows=tx.query("SELECT to_jsonb(p)-'package'-'created_by' FROM analysis_profile_versions p WHERE ($1::text IS NULL OR id>$1) ORDER BY id LIMIT $2",&[&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let items: Vec<Value> = rows.iter().take(limit as usize).map(|r| r.get(0)).collect();
        let next = if rows.len() > limit as usize {
            items
                .last()
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(json!({"items":items,"next_after":next}))
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
        export: bool,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        match PlatformAuthority::require(&mut tx, ctx, Capability::AnalysisProfilesRead, None) {
            Ok(()) => {}
            Err(PlatformError::Forbidden) => Self::admin(&mut tx, ctx)?,
            Err(error) => return Err(error),
        }
        let summary = Self::summary(&mut tx, id)?;
        let package = Self::package(&mut tx, id)?;
        let mut result = summary;
        if export {
            result = serde_json::to_value(package).map_err(|_| PlatformError::Storage)?
        } else {
            result["package"] = serde_json::to_value(package).map_err(|_| PlatformError::Storage)?
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    fn package(
        tx: &mut Transaction<'_>,
        id: &str,
    ) -> Result<AnalysisProfilePackage, PlatformError> {
        let row = tx
            .query_opt(
                "SELECT package,checksum FROM analysis_profile_versions WHERE id=$1",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let package: AnalysisProfilePackage = PlatformStore::decode(row.get(0))?;
        if package.checksum().map_err(|_| PlatformError::Storage)? != row.get::<_, String>(1) {
            return Err(PlatformError::Storage);
        }
        Ok(package)
    }
    fn assignment_value(
        tx: &mut Transaction<'_>,
        repo: &str,
        can_manage: bool,
    ) -> Result<Value, PlatformError> {
        let row=tx.query_opt("SELECT revision,profile_version_id,mode,profile_version_ids FROM repository_analysis_profiles WHERE repository_id=$1",&[&repo]).map_err(PlatformStore::database_error)?;
        let revision = row.as_ref().map(|r| r.get::<_, i64>(0)).unwrap_or(0);
        let id = row.as_ref().and_then(|r| r.get::<_, Option<String>>(1));
        let profile = id.map(|id| Self::summary(tx, &id)).transpose()?;
        let mode = row
            .as_ref()
            .map(|r| r.get::<_, String>(2))
            .unwrap_or_else(|| "automatic".into());
        let ids: Value = row.as_ref().map(|r| r.get(3)).unwrap_or(json!([]));
        let profiles = ids
            .as_array()
            .ok_or(PlatformError::Storage)?
            .iter()
            .map(|id| Self::summary(tx, id.as_str().ok_or(PlatformError::Storage)?))
            .collect::<Result<Vec<_>, _>>()?;
        let last_selection: Option<Value> = None;
        Ok(
            json!({"repository_id":repo,"revision":revision,"profile":profile,"profiles":profiles,"mode":mode,"can_manage":can_manage,"last_selection":last_selection}),
        )
    }

    pub fn assignment(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repo: &str,
        input: Option<&RepositoryAnalysisProfileChange>,
    ) -> Result<Value, PlatformError> {
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if input.is_some() {
            Self::admin(&mut tx, ctx)?;
            tx.query_opt(
                "SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 FOR UPDATE",
                &[&repo, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        }
        PlatformAuthority::require(&mut tx, ctx, Capability::RepositoriesRead, Some(project))
            .map_err(|e| {
                if matches!(e, PlatformError::Forbidden) {
                    PlatformError::NotFound
                } else {
                    e
                }
            })?;
        PlatformAuthority::project(&mut tx, project)?;
        tx.query_opt(
            "SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND NOT archived",
            &[&repo, &project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let can_manage = ctx.method() == AuthMethod::Session
            && match PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)
            {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(e) => return Err(e),
            };
        if let Some(input) = input {
            if input.expected_revision < 0 {
                return Err(PlatformError::Validation("invalid assignment revision"));
            }
            let reason = DomainText::required(&input.reason, 4000)?;
            tx.query_one(
                "SELECT id FROM project_repositories WHERE id=$1 FOR UPDATE",
                &[&repo],
            )
            .map_err(PlatformStore::database_error)?;
            if let Some(v) = ReceiptWriter::read(
                &mut tx,
                &store.namespace,
                receipt.as_ref(),
                ctx.actor_id(),
                "repository.analysis_profile.change",
            )? {
                tx.commit().map_err(PlatformStore::database_error)?;
                return Ok(v);
            }
            let current = Self::assignment_value(&mut tx, repo, can_manage)?;
            if current["revision"].as_i64() != Some(input.expected_revision) {
                return Err(PlatformError::Conflict(
                    "repository analysis profile changed",
                ));
            }
            let (mode, ids)=crate::analysis_profile_selection_store::AnalysisProfileSelectionStore::configuration(&mut tx,input)?;
            let scalar = if ids.len() == 1 {
                Some(ids[0].clone())
            } else {
                None
            };
            let profile_ids = json!(ids);
            tx.execute("INSERT INTO repository_analysis_profiles(repository_id,profile_version_id,revision,changed_by,mode,profile_version_ids) VALUES($1,$2,1,$3,$4,$5) ON CONFLICT(repository_id) DO UPDATE SET profile_version_id=EXCLUDED.profile_version_id,mode=EXCLUDED.mode,profile_version_ids=EXCLUDED.profile_version_ids,revision=repository_analysis_profiles.revision+1,changed_by=EXCLUDED.changed_by,changed_at=clock_timestamp()",&[&repo,&scalar,&ctx.actor_id(),&mode,&profile_ids]).map_err(PlatformStore::database_error)?;
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "repository.analysis_profile.changed",
                Some(repo),
                json!({"project_id":project,"profile_version_id":scalar,"profile_version_ids":profile_ids,"mode":mode,"reason":reason}),
            )?;
            let result = Self::assignment_value(&mut tx, repo, can_manage)?;
            ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(result);
        }
        let mut result = Self::assignment_value(&mut tx, repo, can_manage)?;
        match crate::repository_binding_store::RepositoryBindingStore::lock(
            &mut tx, ctx, project, repo,
        ) {
            Ok(binding) => {
                result["last_source_commit"] = json!(binding.pinned_commit);
                result["last_selection"] = tx
                    .query_opt(
                        "SELECT analysis_profile_selection FROM repository_jobs WHERE id=$1",
                        &[&binding.job_id],
                    )
                    .map_err(PlatformStore::database_error)?
                    .and_then(|r| r.get::<_, Option<Value>>(0))
                    .unwrap_or(Value::Null);
            }
            Err(PlatformError::NotFound | PlatformError::Forbidden) => {}
            Err(error) => return Err(error),
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn job(
        store: &mut PlatformStore,
        lease: &RepositoryJobLease,
    ) -> Result<Option<AnalysisProfilePackage>, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let row = RepositoryJobExecutionAuthority::lock(&mut tx, lease, false)?;
        let value: Value = row.get(0);
        let package = value["analysis_profile_version_id"]
            .as_str()
            .map(|id| Self::package(&mut tx, id))
            .transpose()?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(package)
    }
}
