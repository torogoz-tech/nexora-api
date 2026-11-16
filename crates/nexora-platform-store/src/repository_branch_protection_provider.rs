use crate::pull_request_client::PullRequestClient;
use nexora_platform::{GitProvider, PlatformError};
use serde_json::{Value, json};

/// Reconciles provider protections without granting Nexora a bypass identity.
/// Human identity eligibility remains an additional Nexora merge-time gate.
pub(crate) struct RepositoryBranchProtectionProvider;
impl RepositoryBranchProtectionProvider {
    pub(crate) fn preview(
        client: &PullRequestClient,
        provider: GitProvider,
        root: &str,
        token: &str,
        checks: &[String],
    ) -> Result<Value, PlatformError> {
        if provider == GitProvider::Github
            && (checks.is_empty() || checks.iter().any(|check| check.trim().is_empty()))
        {
            return Err(PlatformError::Validation(
                "Explicit nonempty required CI contexts are required for GitHub",
            ));
        }
        let branches = ["main", "develop", "stage", "preprod"];
        // Complete this preflight before constructing any mutation.
        for branch in branches {
            let path = match provider {
                GitProvider::Github => format!("{root}/branches/{branch}"),
                GitProvider::Gitlab => format!("{root}/repository/branches/{branch}"),
            };
            client.get(&path, token).map_err(|error| match error {
                PlatformError::NotFound => PlatformError::Conflict("All four base branches must exist before protection; bootstrap missing branches separately"),
                other => other,
            })?;
        }
        let mut operations = Vec::new();
        if provider == GitProvider::Gitlab {
            Self::gitlab_open_merge_requests(client, root, token)?;
            // GitLab combines matching branch rules permissively. Never assert safety
            // based only on the exact rule if a wildcard or inherited rule also applies.
            let rules = Self::list(client, &format!("{root}/protected_branches"), token)?;
            if rules.iter().any(|rule| {
                rule["name"].as_str().is_none_or(|name| name.contains('*'))
                    || rule["inherited"] == true
            }) {
                return Err(PlatformError::Conflict(
                    "Inherited or wildcard GitLab protections require administrator review before reconciliation",
                ));
            }
            let project = client.get(root, token)?;
            if project["only_allow_merge_if_pipeline_succeeds"] != true
                || project["allow_merge_on_skipped_pipeline"] != false
            {
                operations.push(json!({"method":"PUT","path":root,"body":{"only_allow_merge_if_pipeline_succeeds":true,"allow_merge_on_skipped_pipeline":false},"expected":{"only_allow_merge_if_pipeline_succeeds":true,"allow_merge_on_skipped_pipeline":false}}));
            }
            let path = format!("{root}/approvals");
            let approvals = client.get(&path, token).map_err(Self::capability_error)?;
            let expected = json!({"merge_requests_author_approval":false,"merge_requests_disable_committers_approval":true,"disable_overriding_approvers_per_merge_request":true,"reset_approvals_on_push":true});
            if !Self::contains(&approvals, &expected) {
                operations
                    .push(json!({"method":"POST","path":path,"body":expected,"expected":expected}));
            }
            let approval_rules = Self::list(client, &format!("{root}/approval_rules"), token)
                .map_err(Self::capability_error)?;
            // Add an independent all-branches baseline; never edit third-party rules.
            let existing = approval_rules
                .iter()
                .find(|rule| rule["name"] == "Nexora independent approval");
            if let Some(existing) = existing {
                if existing["approvals_required"].as_u64().unwrap_or(0) < 1
                    || existing["applies_to_all_protected_branches"] != true
                {
                    return Err(PlatformError::Conflict(
                        "Existing Nexora approval rule requires administrator reconciliation",
                    ));
                }
            } else {
                operations.push(json!({"method":"POST","path":format!("{root}/approval_rules"),"body":{"name":"Nexora independent approval","approvals_required":1,"rule_type":"any_approver","applies_to_all_protected_branches":true},"verify_list":true,"expected":{"name":"Nexora independent approval","approvals_required":1,"applies_to_all_protected_branches":true}}));
            }
        }
        for branch in branches {
            let path = match provider {
                GitProvider::Github => format!("{root}/branches/{branch}/protection"),
                GitProvider::Gitlab => format!("{root}/protected_branches/{branch}"),
            };
            let current = match client.get(&path, token) {
                Ok(value) => value,
                Err(PlatformError::NotFound) => Value::Null,
                Err(error) => return Err(error),
            };
            match provider {
                GitProvider::Github => {
                    let body = Self::github_body(&current, checks)?;
                    let actual = Self::github_projection(&current)?;
                    if actual != body {
                        operations.push(json!({"method":"PUT","path":path,"body":body,"github":true,"expected":body}));
                    }
                }
                GitProvider::Gitlab => {
                    let expected =
                        json!({"allow_force_push":false,"push_access_levels":[{"access_level":0}]});
                    if !Self::gitlab_push_blocked(&current)
                        || !Self::gitlab_merge_restricted(&current)
                        || current["allow_force_push"] != false
                    {
                        let mut body = json!({"allow_force_push":false});
                        let (method, write_path) = if current.is_null() {
                            body["name"] = json!(branch);
                            body["allowed_to_push"] = json!([{"access_level":0}]);
                            body["allowed_to_merge"] = json!([{"access_level":40}]);
                            ("POST", format!("{root}/protected_branches"))
                        } else {
                            let entries = current["push_access_levels"]
                                .as_array()
                                .ok_or(PlatformError::Unavailable)?;
                            let mut replacement = Vec::new();
                            for entry in entries {
                                let id = entry["id"].as_u64().ok_or(PlatformError::Conflict("Provider omitted access record IDs; refusing destructive protection replacement"))?;
                                replacement.push(json!({"id":id,"_destroy":true}));
                            }
                            replacement.push(json!({"access_level":0}));
                            body["allowed_to_push"] = json!(replacement);
                            let mut merge_updates = Vec::new();
                            for entry in current["merge_access_levels"]
                                .as_array()
                                .ok_or(PlatformError::Unavailable)?
                            {
                                if entry["access_level"]
                                    .as_u64()
                                    .is_some_and(|level| level > 0 && level < 40)
                                    && entry["user_id"].is_null()
                                    && entry["group_id"].is_null()
                                {
                                    let id =
                                        entry["id"].as_u64().ok_or(PlatformError::Unavailable)?;
                                    merge_updates.push(json!({"id":id,"access_level":40}));
                                }
                            }
                            if !merge_updates.is_empty() {
                                body["allowed_to_merge"] = json!(merge_updates);
                            }
                            ("PATCH", path.clone())
                        };
                        operations.push(json!({"method":method,"path":write_path,"verify_path":path,"body":body,"gitlab_branch":true,"expected":expected}));
                    }
                }
            }
        }
        Ok(
            json!({"provider":provider.as_str(),"branches":branches.map(|name| json!({"name":name,"status":"existing","verified":false})),"operations":operations,"verified":false,"requires_human_merge_gate":true,"limitations":if provider == GitProvider::Gitlab {json!(["GitLab owners and maintainers with unprotect permission can delete branches through the UI/API; no absolute administrator deletion guarantee"])} else {json!([])}}),
        )
    }

    pub(crate) fn apply(
        client: &PullRequestClient,
        provider: GitProvider,
        root: &str,
        token: &str,
        checks: &[String],
    ) -> Result<Value, PlatformError> {
        let plan = Self::preview(client, provider, root, token, checks)?;
        let operations = plan["operations"]
            .as_array()
            .ok_or(PlatformError::Unavailable)?;
        for operation in operations {
            let path = operation["path"]
                .as_str()
                .ok_or(PlatformError::Unavailable)?;
            let result = client.protection_write(
                operation["method"]
                    .as_str()
                    .ok_or(PlatformError::Unavailable)?,
                path,
                token,
                &operation["body"],
            );
            // A lost response must never cause a blind second write. Read back first.
            let verify_path = operation["verify_path"].as_str().unwrap_or(path);
            let actual = client.get(verify_path, token)?;
            let matches = if operation["github"] == true {
                Self::github_verified(&actual, &operation["expected"])?
            } else if operation["gitlab_branch"] == true {
                Self::gitlab_push_blocked(&actual)
                    && Self::gitlab_merge_restricted(&actual)
                    && actual["allow_force_push"] == false
            } else if operation["verify_list"] == true {
                Self::list(client, verify_path, token)?
                    .iter()
                    .any(|rule| Self::contains(rule, &operation["expected"]))
            } else {
                Self::contains(&actual, &operation["expected"])
            };
            if !matches {
                return Err(result.err().unwrap_or(PlatformError::Conflict("Provider did not retain requested protection; inspect partial application before retry")));
            }
        }
        let mut verified = Self::preview(client, provider, root, token, checks)?;
        if !verified["operations"]
            .as_array()
            .is_some_and(|operations| operations.is_empty())
        {
            return Err(PlatformError::Conflict(
                "Remote protection verification failed; partial application requires review",
            ));
        }
        verified["verified"] = json!(true);
        for branch in verified["branches"]
            .as_array_mut()
            .ok_or(PlatformError::Unavailable)?
        {
            branch["verified"] = json!(true);
        }
        verified["applied_operations"] = json!(operations.len());
        Ok(verified)
    }

    fn capability_error(error: PlatformError) -> PlatformError {
        match error {
            PlatformError::NotFound | PlatformError::Forbidden => PlatformError::Conflict(
                "GitLab approval controls unavailable; Premium/Ultimate and administrator permissions are required",
            ),
            other => other,
        }
    }
    fn list(
        client: &PullRequestClient,
        path: &str,
        token: &str,
    ) -> Result<Vec<Value>, PlatformError> {
        let mut values = Vec::new();
        for page in 1..=100 {
            let separator = if path.contains('?') { "&" } else { "?" };
            let response =
                client.get(&format!("{path}{separator}per_page=100&page={page}"), token)?;
            let entries = response.as_array().ok_or(PlatformError::Unavailable)?;
            values.extend(entries.iter().cloned());
            if entries.len() < 100 {
                return Ok(values);
            }
        }
        Err(PlatformError::Conflict(
            "Protection pagination exceeded safe inspection limit",
        ))
    }
    fn contains(actual: &Value, expected: &Value) -> bool {
        match expected {
            Value::Object(fields) => fields
                .iter()
                .all(|(key, value)| Self::contains(&actual[key], value)),
            _ => actual == expected,
        }
    }
    fn gitlab_open_merge_requests(
        client: &PullRequestClient,
        root: &str,
        token: &str,
    ) -> Result<(), PlatformError> {
        let requests = Self::list(
            client,
            &format!("{root}/merge_requests?state=opened"),
            token,
        )?;
        for request in requests {
            let target = request["target_branch"]
                .as_str()
                .ok_or(PlatformError::Unavailable)?;
            if !["main", "develop", "stage", "preprod"].contains(&target) {
                continue;
            }
            let iid = request["iid"]
                .as_u64()
                .filter(|iid| *iid > 0)
                .ok_or(PlatformError::Unavailable)?;
            let state = client
                .get(
                    &format!("{root}/merge_requests/{iid}/approval_state"),
                    token,
                )
                .map_err(Self::capability_error)?;
            let guarded = state["approval_rules_overwritten"] == false
                && state["rules"].as_array().is_some_and(|rules| {
                    rules.iter().any(|rule| {
                        rule["approvals_required"]
                            .as_u64()
                            .is_some_and(|count| count >= 1)
                            && rule["overridden"] == false
                    })
                });
            if !guarded {
                return Err(PlatformError::Conflict(
                    "Existing open merge requests targeting protected branches require approval-rule review before protection can be applied",
                ));
            }
        }
        Ok(())
    }
    fn gitlab_push_blocked(actual: &Value) -> bool {
        actual["push_access_levels"]
            .as_array()
            .is_some_and(|entries| {
                !entries.is_empty()
                    && entries.iter().all(|entry| {
                        entry["access_level"] == 0
                            && ["user_id", "group_id", "deploy_key_id", "member_role_id"]
                                .iter()
                                .all(|key| entry[key].is_null())
                    })
            })
    }
    fn gitlab_merge_restricted(actual: &Value) -> bool {
        actual["merge_access_levels"]
            .as_array()
            .is_some_and(|entries| {
                entries.iter().all(|entry| {
                    !entry["user_id"].is_null()
                        || !entry["group_id"].is_null()
                        || entry["access_level"]
                            .as_u64()
                            .is_some_and(|level| level == 0 || level >= 40)
                })
            })
    }
    fn github_verified(actual: &Value, expected: &Value) -> Result<bool, PlatformError> {
        let mut actual = Self::github_projection(actual)?;
        // GitHub may bind a newly requested context to the app that last supplied
        // it. This is stronger than our unspecified binding. Existing app pins
        // remain exact requirements and are never removed or widened.
        if let (Some(actual_checks), Some(expected_checks)) = (
            actual["required_status_checks"]["checks"].as_array_mut(),
            expected["required_status_checks"]["checks"].as_array(),
        ) {
            for check in actual_checks {
                let unbound = expected_checks.iter().any(|expected_check| {
                    expected_check["context"] == check["context"]
                        && expected_check.get("app_id").is_none()
                });
                if unbound
                    && check["app_id"].as_i64().is_some_and(|id| id > 0)
                    && let Some(fields) = check.as_object_mut()
                {
                    fields.remove("app_id");
                }
            }
        }
        Ok(actual == *expected)
    }
    fn github_body(current: &Value, checks: &[String]) -> Result<Value, PlatformError> {
        let mut body = Self::github_projection(current)?;
        if body.is_null() {
            body = json!({"required_status_checks":{"strict":true,"checks":[]},"enforce_admins":true,"required_pull_request_reviews":{"required_approving_review_count":1,"dismiss_stale_reviews":true,"require_last_push_approval":true,"require_code_owner_reviews":false,"dismissal_restrictions":{"users":[],"teams":[],"apps":[]},"bypass_pull_request_allowances":{"users":[],"teams":[],"apps":[]}},"restrictions":null,"required_linear_history":false,"block_creations":false,"required_conversation_resolution":false,"lock_branch":false,"allow_fork_syncing":false,"allow_force_pushes":false,"allow_deletions":false});
        }
        if body["required_status_checks"].is_null() {
            body["required_status_checks"] = json!({"strict":true,"checks":[]});
        }
        let status = &mut body["required_status_checks"];
        status["strict"] = json!(true);
        let existing = status["checks"]
            .as_array_mut()
            .ok_or(PlatformError::Unavailable)?;
        for check in checks {
            if !existing
                .iter()
                .any(|entry| entry["context"].as_str() == Some(check))
            {
                existing.push(json!({"context":check}));
            }
        }
        if body["required_pull_request_reviews"].is_null() {
            body["required_pull_request_reviews"] = json!({});
        }
        let reviews = &mut body["required_pull_request_reviews"];
        reviews["required_approving_review_count"] = json!(
            reviews["required_approving_review_count"]
                .as_u64()
                .unwrap_or(0)
                .max(1)
        );
        reviews["dismiss_stale_reviews"] = json!(true);
        reviews["require_last_push_approval"] = json!(true);
        reviews["bypass_pull_request_allowances"] = json!({"users":[],"teams":[],"apps":[]});
        body["enforce_admins"] = json!(true);
        body["allow_force_pushes"] = json!(false);
        body["allow_deletions"] = json!(false);
        Ok(body)
    }
    fn github_projection(current: &Value) -> Result<Value, PlatformError> {
        if current.is_null() {
            return Ok(Value::Null);
        }
        let known = [
            "url",
            "required_status_checks",
            "enforce_admins",
            "required_pull_request_reviews",
            "restrictions",
            "required_linear_history",
            "allow_force_pushes",
            "allow_deletions",
            "block_creations",
            "required_conversation_resolution",
            "lock_branch",
            "allow_fork_syncing",
            "required_signatures",
        ];
        if current
            .as_object()
            .is_none_or(|fields| fields.keys().any(|key| !known.contains(&key.as_str())))
        {
            return Err(PlatformError::Conflict(
                "Unknown GitHub protection fields require review to preserve stronger rules",
            ));
        }
        let mut body = json!({"enforce_admins":false,"required_linear_history":false,"allow_force_pushes":false,"allow_deletions":false,"block_creations":false,"required_conversation_resolution":false,"lock_branch":false,"allow_fork_syncing":false});
        for key in [
            "enforce_admins",
            "required_linear_history",
            "allow_force_pushes",
            "allow_deletions",
            "block_creations",
            "required_conversation_resolution",
            "lock_branch",
            "allow_fork_syncing",
        ] {
            if let Some(value) = current.get(key) {
                body[key] = value.get("enabled").unwrap_or(value).clone();
            }
        }
        let status = &current["required_status_checks"];
        body["required_status_checks"] = if status.is_null() {
            Value::Null
        } else {
            let mut checks = status["checks"].as_array().cloned().unwrap_or_default();
            for check in &mut checks {
                if check["app_id"].is_null() {
                    check
                        .as_object_mut()
                        .ok_or(PlatformError::Unavailable)?
                        .remove("app_id");
                }
            }
            if let Some(contexts) = status["contexts"].as_array() {
                for context in contexts {
                    if !checks.iter().any(|check| check["context"] == *context) {
                        checks.push(json!({"context":context}));
                    }
                }
            }
            json!({"strict":status["strict"],"checks":checks})
        };
        body["restrictions"] = Self::github_actors(&current["restrictions"])?;
        let reviews = &current["required_pull_request_reviews"];
        body["required_pull_request_reviews"] = if reviews.is_null() {
            Value::Null
        } else {
            let mut result = json!({"dismiss_stale_reviews":false,"require_code_owner_reviews":false,"required_approving_review_count":0,"require_last_push_approval":false,"dismissal_restrictions":{"users":[],"teams":[],"apps":[]},"bypass_pull_request_allowances":{"users":[],"teams":[],"apps":[]}});
            let known = [
                "url",
                "dismissal_restrictions",
                "dismiss_stale_reviews",
                "require_code_owner_reviews",
                "required_approving_review_count",
                "require_last_push_approval",
                "bypass_pull_request_allowances",
            ];
            if reviews
                .as_object()
                .is_none_or(|fields| fields.keys().any(|key| !known.contains(&key.as_str())))
            {
                return Err(PlatformError::Conflict(
                    "Unknown GitHub review controls require review",
                ));
            }
            for key in [
                "dismiss_stale_reviews",
                "require_code_owner_reviews",
                "required_approving_review_count",
                "require_last_push_approval",
            ] {
                if let Some(value) = reviews.get(key) {
                    result[key] = value.clone();
                }
            }
            for key in ["dismissal_restrictions", "bypass_pull_request_allowances"] {
                if let Some(value) = reviews.get(key) {
                    result[key] = Self::github_actors(value)?;
                }
            }
            result
        };
        Ok(body)
    }
    fn github_actors(value: &Value) -> Result<Value, PlatformError> {
        if value.is_null() {
            return Ok(Value::Null);
        }
        let mut result = json!({});
        for (key, field) in [("users", "login"), ("teams", "slug"), ("apps", "slug")] {
            let actors = value[key].as_array().cloned().unwrap_or_default();
            let mut names = Vec::new();
            for actor in actors {
                names.push(
                    actor
                        .as_str()
                        .or_else(|| actor[field].as_str())
                        .ok_or(PlatformError::Unavailable)?
                        .to_owned(),
                );
            }
            result[key] = json!(names);
        }
        Ok(result)
    }
}
#[cfg(test)]
#[path = "repository_branch_protection_provider_tests.rs"]
mod tests;
