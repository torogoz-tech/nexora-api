use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpProjectCreate {
    pub request_id: String,
    pub definition: crate::create_project_request::CreateProjectRequest,
}
