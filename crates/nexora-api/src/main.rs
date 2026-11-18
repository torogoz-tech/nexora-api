use clap::Parser;
use nexora_api::{ApiArguments, ApiCommand, ApiServer, ApiState, BootstrapPassword, McpHeaders};
use nexora_platform_store::PlatformStore;
use nexora_runtime::DatabaseConnections;
use std::process::ExitCode;
#[tokio::main]
async fn main() -> ExitCode {
    match run(ApiArguments::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nexora-api: {error}");
            ExitCode::FAILURE
        }
    }
}
async fn run(args: ApiArguments) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        ApiCommand::AdoptLocal(input) => {
            let config = DatabaseConnections::load(&args.connections)?;
            let value = tokio::task::spawn_blocking(move || {
                nexora_api::LocalSnapshotCommands::adopt(&config, &args.schema, input)
            })
            .await??;
            println!("{}", serde_json::to_string(&value)?);
        }
        ApiCommand::RevokeLocal(input) => {
            let config = DatabaseConnections::load(&args.connections)?;
            let value = tokio::task::spawn_blocking(move || {
                nexora_api::LocalSnapshotCommands::revoke(&config, &args.schema, input)
            })
            .await??;
            println!("{}", serde_json::to_string(&value)?);
        }
        ApiCommand::ReauthorizeLocal(input) => {
            let config = DatabaseConnections::load(&args.connections)?;
            let value = tokio::task::spawn_blocking(move || {
                nexora_api::LocalSnapshotCommands::reauthorize(&config, &args.schema, input)
            })
            .await??;
            println!("{}", serde_json::to_string(&value)?);
        }
        ApiCommand::McpHeaders { api_key_file } => {
            let headers = McpHeaders::read(&api_key_file)?;
            println!("{}", headers.as_str());
        }
        ApiCommand::Migrate => {
            let config = DatabaseConnections::load(&args.connections)?;
            tokio::task::spawn_blocking(move || {
                let mut store = PlatformStore::connect(&config, &args.schema)?;
                store.initialize()
            })
            .await??;
            println!("{}", serde_json::json!({"platform_schema":"ready"}));
        }
        ApiCommand::Bootstrap {
            organization,
            username,
            name,
            password_file,
        } => {
            let config = DatabaseConnections::load(&args.connections)?;
            let result = tokio::task::spawn_blocking(move || {
                let password = BootstrapPassword::read(&password_file)?;
                let mut store = PlatformStore::connect(&config, &args.schema)?;
                store.initialize()?;
                store.bootstrap(&organization, &username, &password, &name)
            })
            .await??;
            println!("{}", serde_json::to_string(&result)?);
        }
        ApiCommand::Serve {
            cors_origin,
            trusted_proxy,
            reasoning_settings,
            bind,
            workers,
            identity_policy,
            git_policy,
        } => {
            let config = DatabaseConnections::load(&args.connections)?;
            let policy = identity_policy
                .as_deref()
                .map(nexora_platform_store::OidcNetworkPolicy::load)
                .transpose()?
                .unwrap_or_default();
            let git_policy = git_policy
                .as_deref()
                .map(nexora_platform_store::GitNetworkPolicy::load)
                .transpose()?
                .unwrap_or_default();
            let reasoning = reasoning_settings
                .as_deref()
                .map(nexora_api::ReasoningSettings::load)
                .transpose()?;
            let state = ApiState::with_policies(config, args.schema, workers, policy, git_policy)
                .await?
                .with_reasoning(reasoning)
                .with_trusted_proxies(trusted_proxy);
            let dispatcher = std::env::var("NEXORA_RABBITMQ_URL")
                .ok()
                .filter(|uri| !uri.trim().is_empty())
                .map(|uri| {
                    let (sender, task) =
                        nexora_api::RabbitMqJobWakeupDispatcher::start(state.clone(), uri);
                    (sender, task)
                });
            let state = match &dispatcher {
                Some((sender, _)) => state.with_job_wakeup_sender(sender.clone()),
                None => {
                    eprintln!(
                        "nexora-api: NEXORA_RABBITMQ_URL is unset; repository jobs will require worker startup recovery"
                    );
                    state
                }
            };
            let cors = nexora_api::ApiCors::layer(&cors_origin)?;
            let listener = tokio::net::TcpListener::bind(bind).await?;
            eprintln!("nexora-api listening on {}", listener.local_addr()?);
            axum::serve(
                listener,
                ApiServer::router(state)
                    .layer(cors)
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_signal())
            .await?;
            if let Some((_, task)) = dispatcher {
                task.abort();
            }
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
