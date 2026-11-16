use crate::ApiState;
use nexora_platform::{InstallationService, OpenAiConfiguration, PlatformError};
use nexora_platform_store::PlatformStore;
use serde_json::json;
pub(crate) struct DailyDeveloperAnalysisWorker;
impl DailyDeveloperAnalysisWorker {
    pub async fn tick(state: &ApiState) {
        if state
            .run(|store| store.schedule_daily_developer_analyses())
            .await
            .is_err()
        {
            return;
        }
        let Ok((settings, config, namespace, permit)) = state.reasoning_context() else {
            return;
        };
        let result=tokio::task::spawn_blocking(move||->Result<(),PlatformError>{
   let _permit=permit;let mut store=PlatformStore::connect(&config,&namespace)?;
   // Resolve service configuration before claiming: unavailable configuration spends nothing.
   let (key,model,effort,endpoint)=if let Some(config)=store.service_runtime_settings(InstallationService::Openai)?{
    let metadata:OpenAiConfiguration=serde_json::from_value(config.settings).map_err(|_|PlatformError::Storage)?;let secret=config.secret.ok_or(PlatformError::Unavailable)?;
    (zeroize::Zeroizing::new(String::from_utf8(secret.to_vec()).map_err(|_|PlatformError::Storage)?),metadata.model,metadata.effort,metadata.endpoint)
   }else{let settings=settings.ok_or(PlatformError::Unavailable)?;(nexora_reasoning::SecretFile::read(&settings.key_file).map_err(|_|PlatformError::Unavailable)?,settings.model,settings.effort,"https://api.openai.com/v1".into())};
   let client=nexora_reasoning::OpenAiClient::new(key,model).and_then(|c|c.with_endpoint(&endpoint)).map_err(|_|PlatformError::Unavailable)?;
   let Some(job)=store.claim_daily_developer_analysis()? else{return Ok(())};
   let input=json!({"date":job["date"],"timezone":job["timezone"],"language":job["language"],"evidence":{"items":job["evidence"]["items"],"counts":job["evidence"]["counts"],"availability":job["evidence"]["availability"]}});
   let report=nexora_reasoning::DailyDeveloperAnalysis::run(&client,&input,&effort).map_err(|_|PlatformError::Unavailable);
   store.finish_daily_developer_analysis(&job,report.as_ref().ok())?;report.map(|_|())
  }).await;
        if !matches!(result, Ok(Ok(()))) {
            eprintln!("daily developer analysis processing unavailable");
        }
    }
}
