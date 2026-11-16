use serde::Deserialize;
#[derive(Deserialize)]
pub(crate) struct GitCallbackQuery {
    pub state: Option<String>,
    pub code: Option<String>,
    pub error: Option<String>,
}
