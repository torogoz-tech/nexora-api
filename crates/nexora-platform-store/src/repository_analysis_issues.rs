use crate::PlatformStore;
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn repository_analysis_issues(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repo: &str,
        offset: usize,
        limit: u16,
    ) -> Result<Value, PlatformError> {
        if !(1..=100).contains(&limit) || offset > 100_000 {
            return Err(PlatformError::Validation("invalid analysis issues page"));
        }
        let assignment = self.repository_analysis_profile(ctx, project, repo)?;
        let selection = &assignment["last_selection"];
        let issues:Vec<Value>=selection["skipped_files"].as_array().into_iter().flatten().cloned().chain(selection["uncovered_files"].as_array().into_iter().flatten().map(|v|json!({"path":v,"reason":"no compatible analysis profile","profile_id":null}))).collect();
        let items: Vec<_> = issues.iter().skip(offset).take(limit as usize).collect();
        let next = if offset.saturating_add(items.len()) < issues.len() {
            Some(offset + items.len())
        } else {
            None
        };
        Ok(
            json!({"repository_id":repo,"commit":assignment["last_source_commit"],"profiles":selection["profiles"].as_array().map(|p|p.iter().map(|v|json!({"id":v["id"],"key":v["key"],"version":v["version"],"checksum":v["checksum"]})).collect::<Vec<_>>()).unwrap_or_default(),"items":items,"next_offset":next,"total":issues.len()}),
        )
    }
}
