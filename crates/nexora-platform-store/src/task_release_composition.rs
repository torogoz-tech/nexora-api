use nexora_platform::GitProvider;
use serde_json::Value;
pub(crate) struct TaskReleaseComposition;
impl TaskReleaseComposition {
    pub fn contains(provider: GitProvider, response: &Value, source_head: &str) -> bool {
        match provider {
            GitProvider::Github => {
                response["merge_base_commit"]["sha"].as_str() == Some(source_head)
            }
            GitProvider::Gitlab => response["id"].as_str() == Some(source_head),
        }
    }
}
#[cfg(test)]
#[path = "task_release_composition_tests.rs"]
mod tests;
