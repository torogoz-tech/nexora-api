use nexora_platform::{GitProvider, PlatformError};
use serde_json::{Value, json};
pub struct PullRequestDecoder;
impl PullRequestDecoder {
    fn text(value: &Value, key: &str, max: usize) -> String {
        value[key]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(max)
            .collect()
    }
    pub fn summary(provider: GitProvider, v: &Value) -> Result<Value, PlatformError> {
        let github = provider == GitProvider::Github;
        let number = v[if github { "number" } else { "iid" }]
            .as_u64()
            .filter(|v| *v > 0 && *v <= i64::MAX as u64)
            .ok_or(PlatformError::Unavailable)?;
        let title = Self::text(v, "title", 512);
        if title.is_empty() {
            return Err(PlatformError::Unavailable);
        }
        let merged = if github {
            v["merged"].as_bool() == Some(true) || v["merged_at"].is_string()
        } else {
            v["state"] == "merged"
        };
        let state = if merged {
            "merged"
        } else if ["closed", "locked"].contains(&v["state"].as_str().unwrap_or("")) {
            "closed"
        } else {
            "open"
        };
        Ok(
            json!({"number":number,"title":title,"description":Self::text(v,if github{"body"}else{"description"},65536),"state":state,"draft":v["draft"].as_bool().unwrap_or(false),"web_url":Self::text(v,if github{"html_url"}else{"web_url"},2048),"author":if github{Self::text(&v["user"],"login",256)}else{Self::text(&v["author"],"username",256)},"source_branch":if github{Self::text(&v["head"],"ref",512)}else{Self::text(v,"source_branch",512)},"target_branch":if github{Self::text(&v["base"],"ref",512)}else{Self::text(v,"target_branch",512)},"head_sha":if github{Self::text(&v["head"],"sha",64)}else{Self::text(v,"sha",64)},"base_sha":if github{Self::text(&v["base"],"sha",64)}else{Self::text(&v["diff_refs"],"base_sha",64)},"changed_files":v["changed_files"].as_u64(),"created_at":Self::text(v,"created_at",64),"updated_at":Self::text(v,"updated_at",64),"mergeable":v["mergeable"].as_bool(),"additions":v["additions"].as_u64(),"deletions":v["deletions"].as_u64()}),
        )
    }
    pub fn file(provider: GitProvider, v: &Value) -> Result<Value, PlatformError> {
        let github = provider == GitProvider::Github;
        let path = Self::text(v, if github { "filename" } else { "new_path" }, 4096);
        if path.is_empty() {
            return Err(PlatformError::Unavailable);
        }
        let raw = v[if github { "patch" } else { "diff" }].as_str();
        let patch = raw.map(|s| s.chars().take(32768).collect::<String>());
        let status = if github {
            Self::text(v, "status", 64)
        } else if v["new_file"] == true {
            "added".into()
        } else if v["deleted_file"] == true {
            "removed".into()
        } else if v["renamed_file"] == true {
            "renamed".into()
        } else {
            "modified".into()
        };
        Ok(
            json!({"path":path,"previous_path":Self::text(v,if github{"previous_filename"}else{"old_path"},4096),"status":status,"additions":v["additions"].as_u64(),"deletions":v["deletions"].as_u64(),"patch":patch,"patch_truncated":raw.is_none()||raw.is_some_and(|s|s.chars().count()>32768)||v["too_large"]==true||v["collapsed"]==true}),
        )
    }
    pub fn comment(provider: GitProvider, v: &Value, inline: bool) -> Result<Value, PlatformError> {
        let id = v["id"].as_u64().ok_or(PlatformError::Unavailable)?;
        Ok(
            json!({"id":format!("{}-{id}",if inline{"inline"}else{"comment"}),"author":if provider==GitProvider::Github{Self::text(&v["user"],"login",256)}else{Self::text(&v["author"],"username",256)},"body":Self::text(v,"body",32768),"created_at":Self::text(v,"created_at",64),"path":if inline{v["path"].as_str()}else{None},"line":v["line"].as_u64()}),
        )
    }
    pub fn check(v: &Value, provider: GitProvider) -> Value {
        json!({"name":v["name"].as_str().or(v["context"].as_str()).map(str::to_owned).unwrap_or_else(||format!("Pipeline #{}",v["id"].as_u64().unwrap_or(0))),"status":v["status"].as_str().or(v["state"].as_str()).unwrap_or("unknown"),"conclusion":v["conclusion"].as_str(),"web_url":v["html_url"].as_str().or(v["details_url"].as_str()).or(v["target_url"].as_str()).or(v["web_url"].as_str()).unwrap_or(""),"provider":provider.as_str()})
    }
}
