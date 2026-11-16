use serde::Deserialize;
/// Unknown provider response parameters carry no authority and are ignored per OAuth.
#[derive(Deserialize)]
pub(crate) struct OidcCallbackQuery {
    pub state: Option<String>,
    pub code: Option<String>,
    pub error: Option<String>,
}
