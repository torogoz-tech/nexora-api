use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub name: String,
}
