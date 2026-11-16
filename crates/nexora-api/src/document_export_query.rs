use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentExportQuery {
    pub format: Option<String>,
}
