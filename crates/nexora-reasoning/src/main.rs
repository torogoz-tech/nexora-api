mod arguments;
use arguments::Arguments;
use clap::Parser;
use nexora_reasoning::{
    AnalysisRequest, OpenAiClient, ReasoningSession, RegisteredRepository, SecretFile,
};
use serde_json::json;
fn main() -> std::process::ExitCode {
    match run(Arguments::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nexora-reasoning: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn run(args: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(&args.request)?
        .take(300001)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 300000 {
        return Err("request too large".into());
    }
    let request: AnalysisRequest = serde_json::from_slice(&bytes)?;
    request.validate()?;
    let mut repository = RegisteredRepository::new(
        args.port,
        SecretFile::read(&args.nexora_key_file)?,
        args.project.clone(),
        args.repository.clone(),
        args.snapshot,
    )?;
    // Authorization and snapshot pinning happen before the first external request.
    repository.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
    let mut baseline = if let Some(id) = args.baseline_snapshot {
        let mut source = RegisteredRepository::new(
            args.port,
            SecretFile::read(&args.nexora_key_file)?,
            args.project,
            args.repository,
            Some(id),
        )?;
        source.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
        if source.identity() == repository.identity() {
            return Err("baseline must differ from current snapshot".into());
        }
        Some(source)
    } else {
        None
    };
    let mut tools = RegisteredRepository::tools();
    if baseline.is_some() {
        tools.push(nexora_reasoning::ComparisonTool::definition());
    }
    if args.check_scope {
        println!(
            "{}",
            json!({"scope_verified":true,"identity":repository.identity(),"baseline":baseline.as_ref().and_then(RegisteredRepository::identity)})
        );
        return Ok(());
    }
    let client = OpenAiClient::new(SecretFile::read(&args.openai_key_file)?, args.model.clone())?;
    let session = ReasoningSession::new(client).with_effort(args.effort.clone())?;
    let (report, evidence) = session.evaluate(&request, tools, |tool, input, timeout| {
        if tool == nexora_reasoning::ComparisonTool::NAME {
            repository.compare_with(
                baseline
                    .as_mut()
                    .ok_or(nexora_reasoning::ReasoningError::Repository)?,
                input,
                timeout,
            )
        } else {
            repository.read_with_timeout(tool, input, timeout)
        }
    })?;
    // A revoked key/source while the provider was reasoning invalidates the result.
    repository.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
    if let Some(source) = &mut baseline {
        source.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
    }
    println!(
        "{}",
        json!({"provider":"openai","model":args.model,"reasoning_effort":args.effort,"identity":repository.identity(),"baseline":baseline.as_ref().and_then(RegisteredRepository::identity),"request":request,"report":report,"evidence":evidence,"authority":"inferred_not_approved"})
    );
    Ok(())
}
