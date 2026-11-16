use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmailActionTokenInput {
    pub token: String,
}

impl Drop for EmailActionTokenInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.token.zeroize();
    }
}
