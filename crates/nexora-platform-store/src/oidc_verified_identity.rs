pub(crate) struct OidcVerifiedIdentity {
    pub subject: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
}
