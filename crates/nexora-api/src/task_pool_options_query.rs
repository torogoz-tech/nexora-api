use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPoolOptionsQuery {
    pub project_id: Option<String>,
}
