use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PasswordRecoveryComplete {
    pub token: String,
    pub password: String,
}

impl Drop for PasswordRecoveryComplete {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.token.zeroize();
        self.password.zeroize();
    }
}
