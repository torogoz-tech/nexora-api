use nexora_platform::ActorKind;
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateActorRequest {
    pub kind: ActorKind,
    pub name: String,
    pub purpose: String,
}
