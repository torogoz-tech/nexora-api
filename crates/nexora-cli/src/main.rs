mod arguments;
mod command;
mod corpus_command;
mod database_runner;
mod governance_check_arguments;
mod governance_command;
mod governance_report_arguments;
mod governance_runner;
mod graph_command;
mod index_session_pass;
mod index_session_report;
mod index_session_runner;
mod json_output;
mod language_command;
mod language_selection;
mod mcp_command;
mod policy_command;
mod policy_test_case;
mod policy_test_result;
mod policy_test_runner;
mod policy_test_suite;
mod report_format;
mod snapshot_command;
mod storage_command;
mod unknown_mode;
use arguments::Arguments;
use clap::Parser;
use command::Command;
use language_command::LanguageCommand;
use nexora_core::TenantId;
use nexora_language::LanguageDefinition;
use std::process::ExitCode;
fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let arguments = Arguments::parse();
    let governance = matches!(
        &arguments.command,
        Command::Governance { .. }
            | Command::Policy { .. }
            | Command::Corpus { .. }
            | Command::IndexSession { .. }
    );
    match run(arguments) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if governance {
                eprintln!("nexora: {}", error.to_string().escape_debug());
            } else {
                eprintln!("nexora: {error}");
            }
            ExitCode::from(if governance { 2 } else { 1 })
        }
    }
}
fn run(arguments: Arguments) -> Result<u8, Box<dyn std::error::Error>> {
    match arguments.command {
        Command::Mcp {
            command:
                mcp_command::McpCommand::Serve {
                    root,
                    tenant,
                    allow_index,
                    allow_knowledge,
                    knowledge_actor,
                },
        } => {
            let config = nexora_mcp::McpConfiguration::new(
                &root,
                &tenant,
                &arguments.connections,
                &arguments.language_dir,
                arguments.language,
                allow_index,
            )?;
            let config = if allow_knowledge {
                config.with_knowledge_actor(knowledge_actor.unwrap())?
            } else {
                config
            };
            nexora_mcp::StdioServer::serve(config, std::io::stdin().lock(), std::io::stdout())?;
        }
        Command::Storage {
            command: storage_command::StorageCommand::Check,
        } => {
            let mut runtime = database_runner::DatabaseRunner::connect(&arguments.connections)?;
            runtime.check()?;
            json_output::write_json(
                &serde_json::json!({"postgresql":"connected","dragonfly":"connected","neo4j":"connected"}),
            )?;
        }
        Command::Storage {
            command:
                storage_command::StorageCommand::PrepareQueries {
                    tenant,
                    repository,
                    snapshot,
                },
        } => {
            let mut runtime = database_runner::DatabaseRunner::connect(&arguments.connections)?;
            let id = snapshot.map(nexora_core::SnapshotId);
            runtime.prepare_queries(
                &TenantId(tenant.clone()),
                &nexora_core::RepositoryId(repository.clone()),
                id.as_ref(),
            )?;
            let header = runtime.query_header(
                &TenantId(tenant.clone()),
                &nexora_core::RepositoryId(repository.clone()),
                id.as_ref(),
            )?;
            json_output::write_json(
                &serde_json::json!({"tenant":tenant,"repository":repository,"snapshot":header.id.0,"payload_hash":header.payload_hash,"query_version":nexora_runtime::QuerySnapshot::VERSION,"query_projection":"ready"}),
            )?;
        }
        Command::Snapshot {
            command:
                snapshot_command::SnapshotCommand::Get {
                    tenant,
                    repository,
                    snapshot,
                },
        } => {
            let mut runtime = database_runner::DatabaseRunner::connect(&arguments.connections)?;
            let id = snapshot.map(nexora_core::SnapshotId);
            let value = runtime
                .snapshot(
                    &TenantId(tenant),
                    &nexora_core::RepositoryId(repository),
                    id.as_ref(),
                )?
                .ok_or_else(|| {
                    nexora_core::NexoraError::Storage("published snapshot not found".into())
                })?;
            json_output::write_json(&value)?;
        }
        Command::Graph {
            command:
                graph_command::GraphCommand::Traverse {
                    tenant,
                    repository,
                    snapshot,
                    node,
                    depth,
                    limit,
                },
        } => {
            let mut runtime = database_runner::DatabaseRunner::connect(&arguments.connections)?;
            let result = runtime.traverse(
                &TenantId(tenant),
                &nexora_core::RepositoryId(repository),
                &nexora_core::SnapshotId(snapshot),
                &nexora_core::NodeId(node),
                usize::from(depth),
                usize::from(limit),
            )?;
            json_output::write_json(&result)?;
        }
        Command::IndexSession {
            path,
            tenant,
            passes,
            cache_bytes,
        } => {
            let catalog = nexora_language::LanguageRegistry::load(&arguments.language_dir)?;
            let language = language_selection::LanguageSelection::path(
                &catalog,
                arguments.language.as_deref(),
                &path,
            )?;
            let report = index_session_runner::IndexSessionRunner::run(
                &path,
                TenantId(tenant),
                language_selection::LanguageSelection::indexer(&catalog, language)?,
                passes,
                cache_bytes as usize,
            )?;
            json_output::write_json(&report)?;
        }

        Command::Corpus { command } => match command {
            corpus_command::CorpusCommand::Validate { path, workspace } => {
                let suite = nexora_corpus::CorpusSuite::load(&path, &workspace)?;
                json_output::write_json(&suite.describe())?;
            }
            corpus_command::CorpusCommand::Check {
                path,
                workspace,
                case,
                include_facts,
            } => {
                let suite = nexora_corpus::CorpusSuite::load(&path, &workspace)?;
                let report =
                    nexora_corpus::CorpusRunner::run(&suite, case.as_deref(), include_facts)?;
                json_output::write_json(&report)?;
                return Ok(u8::from(!report.passed));
            }
        },

        Command::Governance { command } => {
            return governance_runner::GovernanceRunner::run(
                command,
                &arguments.language_dir,
                arguments.language.as_deref(),
            );
        }
        Command::Policy { command } => match command {
            policy_command::PolicyCommand::Validate { path } => {
                let registry = nexora_governance::PolicyRegistry::load(&path)?;
                println!("valid: {} policies", registry.policies().count());
            }
            policy_command::PolicyCommand::Test { path } => {
                let catalog = nexora_language::LanguageRegistry::load(&arguments.language_dir)?;
                let results = policy_test_runner::PolicyTestRunner::run(
                    &path,
                    &catalog,
                    arguments.language.as_deref(),
                )?;
                let failed = results.iter().any(|r| !r.passed);
                json_output::write_json(&results)?;
                return Ok(u8::from(failed));
            }
        },
        Command::Languages {
            command:
                LanguageCommand::ExportProfile {
                    catalog,
                    language,
                    key,
                    version,
                    name,
                    output,
                },
        } => {
            let package = nexora_language::AnalysisProfilePackage::from_catalog(
                &catalog, language, key, version, name,
            )?;
            nexora_indexer::AnalysisProfileCompiler::compile(&package)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            serde_json::to_writer(&mut file, &package.canonical())?;
        }
        Command::Languages {
            command: LanguageCommand::Validate { path },
        } => {
            let language = LanguageDefinition::load(&path)?;
            println!(
                "valid: {} {} (grammar schema {})",
                language.manifest.id.0, language.manifest.version, language.grammar.schema_version
            );
        }
        Command::Languages {
            command: LanguageCommand::List,
        } => {
            let catalog = nexora_language::LanguageRegistry::load(&arguments.language_dir)?;
            for language in catalog.languages() {
                println!(
                    "{} {} [{}]",
                    language.manifest.id.0,
                    language.manifest.version,
                    language.manifest.extensions.join(", ")
                );
            }
        }
        Command::Analyze { path } => {
            let catalog = nexora_language::LanguageRegistry::load(&arguments.language_dir)?;
            let language = language_selection::LanguageSelection::path(
                &catalog,
                arguments.language.as_deref(),
                &path,
            )?;
            let result = language_selection::LanguageSelection::indexer(&catalog, language)?
                .analyze(&path)?;
            json_output::write_json(&result)?;
        }
        Command::Index {
            path,
            tenant,
            offline,
            report,
        } => {
            let catalog = nexora_language::LanguageRegistry::load(&arguments.language_dir)?;
            let language = language_selection::LanguageSelection::path(
                &catalog,
                arguments.language.as_deref(),
                &path,
            )?;
            let indexer = language_selection::LanguageSelection::indexer(&catalog, language)?;
            if offline {
                let snapshot = indexer.index(
                    &path,
                    TenantId(tenant),
                    &mut nexora_indexer::MemoryStore::default(),
                )?;
                json_output::write_json(&snapshot)?;
            } else {
                let mut runtime = database_runner::DatabaseRunner::connect(&arguments.connections)?;
                let result = runtime.index(&indexer, &path, TenantId(tenant))?;
                if report {
                    json_output::write_json(&result)?;
                } else {
                    json_output::write_json(&result.snapshot)?;
                }
            }
        }
    }
    Ok(0)
}
