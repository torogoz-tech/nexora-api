use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct SecurityMethodRequest {
    pub method: String,
}
