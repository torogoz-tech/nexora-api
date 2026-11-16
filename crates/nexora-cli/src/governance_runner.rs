use crate::{governance_command::GovernanceCommand, report_format::ReportFormat};
use nexora_core::{RepositoryId, TenantId};
use nexora_governance::*;
use nexora_indexer::MemoryStore;
use nexora_language::LanguageRegistry;
use nexora_storage::Snapshot;
use std::path::Path;
pub struct GovernanceRunner;
impl GovernanceRunner {
    pub fn run(
        command: GovernanceCommand,
        language_dir: &Path,
        explicit: Option<&str>,
    ) -> Result<u8, Box<dyn std::error::Error>> {
        match command {
            GovernanceCommand::Check(args) => {
                let registry = PolicyRegistry::load(&args.policies)?;
                let policies: Vec<_> = registry.policies().cloned().collect();
                let tenant = TenantId(args.tenant);
                let catalog = LanguageRegistry::load(language_dir)?;
                let (snapshot, coverage, language) = if let Some(path) = args.snapshot {
                    let text = GovernanceIo::read(&path, 67_108_864)?;
                    let value: serde_json::Value = serde_json::from_str(&text)?;
                    let (snapshot, coverage, envelope_language): (
                        Snapshot,
                        AnalysisCoverage,
                        Option<String>,
                    ) = if value.get("schema_version").is_some() {
                        let envelope: GovernanceSnapshot = serde_json::from_value(value)?;
                        if envelope.schema_version != 1 {
                            return Err(GovernanceError::Configuration(
                                "unsupported snapshot envelope".into(),
                            )
                            .into());
                        }
                        (
                            envelope.snapshot,
                            envelope.coverage,
                            Some(envelope.language),
                        )
                    } else {
                        (
                            serde_json::from_value(value)?,
                            AnalysisCoverage::default(),
                            None,
                        )
                    };
                    if explicit.is_some()
                        && envelope_language
                            .as_deref()
                            .is_some_and(|id| Some(id) != explicit)
                    {
                        return Err(GovernanceError::Scope(
                            "snapshot language differs from requested scope".into(),
                        )
                        .into());
                    }
                    let paths: Vec<_> = snapshot
                        .sources
                        .keys()
                        .map(std::path::PathBuf::from)
                        .collect();
                    let language = crate::language_selection::LanguageSelection::choose(
                        &catalog,
                        explicit.or(envelope_language.as_deref()),
                        &paths,
                    )?;
                    (snapshot, coverage, language)
                } else {
                    let path = args.path.as_deref().unwrap_or(Path::new("."));
                    let language = crate::language_selection::LanguageSelection::path(
                        &catalog, explicit, path,
                    )?;
                    let snapshot = crate::language_selection::LanguageSelection::indexer(
                        &catalog,
                        language.clone(),
                    )?
                    .index(
                        path,
                        tenant.clone(),
                        &mut MemoryStore::default(),
                    )?;
                    let coverage = AnalysisCoverage::indexed(&snapshot);
                    (snapshot, coverage, language)
                };
                let repository = args
                    .repository
                    .map(RepositoryId)
                    .unwrap_or_else(|| snapshot.repository_id.clone());
                if snapshot.sources.keys().any(|p| {
                    !Path::new(p)
                        .extension()
                        .and_then(|p| p.to_str())
                        .is_some_and(|e| language.manifest.extensions.iter().any(|x| x == e))
                }) {
                    return Err(GovernanceError::Configuration(
                        "snapshot and configured language rules do not match".into(),
                    )
                    .into());
                }
                let mut context = EvaluationContext::new(
                    &snapshot,
                    &tenant,
                    &repository,
                    &language.manifest.id.0,
                    args.organization.as_deref(),
                )?;
                context.coverage = coverage.clone();
                let result = PolicyEvaluator::evaluate(&context, &policies)?;
                if let Some(path) = args.export_snapshot {
                    let envelope = GovernanceSnapshot {
                        schema_version: 1,
                        snapshot: snapshot.clone(),
                        language: language.manifest.id.0.clone(),
                        coverage,
                    };
                    let bytes = serde_json::to_vec_pretty(&envelope)?;
                    if bytes.len() > 67_108_864 {
                        return Err(GovernanceError::Limit(
                            "encoded snapshot export > 64 MiB".into(),
                        )
                        .into());
                    }
                    let parent = path
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(Path::new("."));
                    GovernanceIo::path(parent)?;
                    if let Ok(m) = std::fs::symlink_metadata(&path)
                        && m.file_type().is_symlink()
                    {
                        return Err(GovernanceError::Scope(
                            "snapshot export symlink rejected".into(),
                        )
                        .into());
                    }
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)?;
                    std::io::Write::write_all(&mut file, &bytes)?;
                }
                let mut store = FileFindingRepository::new(&args.store)?;
                store.save(&result)?;
                eprintln!(
                    "evaluation={} repository={} snapshot={}",
                    result.id,
                    result.repository_id.0.escape_debug(),
                    result.snapshot_id.0.escape_debug()
                );
                Self::print(&result, args.format)?;
                Ok(GovernanceReporter::exit_code(
                    &result,
                    args.unknown.override_value(),
                ))
            }
            GovernanceCommand::Report(args) => {
                let tenant = TenantId(args.tenant);
                let store = FileFindingRepository::new(&args.store)?;
                let (evaluation, repository) = if let Some(evaluation) = args.evaluation {
                    (
                        evaluation,
                        RepositoryId(args.repository.ok_or_else(|| {
                            GovernanceError::Configuration(
                                "--repository required with --evaluation".into(),
                            )
                        })?),
                    )
                } else {
                    let latest = store.latest(&tenant)?.ok_or_else(|| {
                        GovernanceError::Configuration(
                            "no evaluation for this tenant; run governance check first".into(),
                        )
                    })?;
                    (
                        latest.evaluation_id,
                        args.repository
                            .map(RepositoryId)
                            .unwrap_or(latest.repository_id),
                    )
                };
                let result = store
                    .get(&tenant, &repository, &evaluation)?
                    .ok_or_else(|| GovernanceError::Configuration("evaluation not found".into()))?;
                if args
                    .snapshot_id
                    .is_some_and(|id| id != result.snapshot_id.0)
                {
                    return Err(GovernanceError::Scope(
                        "evaluation does not match requested snapshot".into(),
                    )
                    .into());
                }
                Self::print(&result, args.format)?;
                Ok(0)
            }
        }
    }
    fn print(result: &EvaluationResult, format: ReportFormat) -> Result<(), GovernanceError> {
        match format {
            ReportFormat::Json => println!("{}", GovernanceReporter::json(result)?),
            ReportFormat::Text => print!("{}", GovernanceReporter::text(result)),
        };
        Ok(())
    }
}
