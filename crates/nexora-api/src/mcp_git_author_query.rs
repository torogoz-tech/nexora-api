use nexora_platform::GitAuthorQuery;
use serde::Deserialize;
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct McpGitAuthorQuery {
    pub query: GitAuthorQuery,
}
