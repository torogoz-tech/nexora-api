use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemberAvatarInput {
    pub image_base64: String,
}
