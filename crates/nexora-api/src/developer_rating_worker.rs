use crate::ApiState;
pub struct DeveloperRatingWorker;
impl DeveloperRatingWorker {
    /// Catch up missed local-midnight closures after a restart, using database time.
    pub fn start(state: ApiState) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if state
                    .run(|store| store.refresh_developer_ratings())
                    .await
                    .is_err()
                {
                    // Never log request bodies or personal assessment evidence.
                    eprintln!("developer rating day closure will retry");
                }
                crate::knowledge_synthesis_worker::KnowledgeSynthesisWorker::tick(&state).await;
                crate::daily_developer_analysis_worker::DailyDeveloperAnalysisWorker::tick(&state)
                    .await;
            }
        })
    }
}
