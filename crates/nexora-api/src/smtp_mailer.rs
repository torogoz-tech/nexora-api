use lettre::message::Mailbox;
use nexora_platform::PlatformError;
/// Parses and validates recipient mailbox values for SMTP configuration forms.
pub(crate) struct SmtpMailer;
impl SmtpMailer {
    pub fn mailbox(value: &str) -> Result<Mailbox, PlatformError> {
        value
            .parse()
            .map_err(|_| PlatformError::Validation("invalid email address"))
    }
}
