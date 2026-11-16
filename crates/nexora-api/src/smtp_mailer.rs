use lettre::{
    Message, SmtpTransport, Transport,
    message::{Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
};
use nexora_platform::{PlatformError, SmtpConfiguration};
use std::time::Duration;
use zeroize::Zeroizing;
/// Server-only SMTP transport. Always verifies TLS; never logs credentials or server replies.
pub(crate) struct SmtpMailer;
impl SmtpMailer {
    pub fn mailbox(value: &str) -> Result<Mailbox, PlatformError> {
        value
            .parse()
            .map_err(|_| PlatformError::Validation("invalid email address"))
    }
    pub fn send(
        config: &SmtpConfiguration,
        secret: Option<&[u8]>,
        recipient: &str,
        subject: &str,
        body: &str,
        id: &str,
        html: &str,
    ) -> Result<(), PlatformError> {
        config.validate()?;
        let from = Mailbox::new(
            Some(config.sender_name.clone()),
            Self::mailbox(&config.sender_email)?.email,
        );
        let message = Message::builder()
            .from(from)
            .to(Self::mailbox(recipient)?)
            .subject(subject)
            .message_id(Some(format!("<{id}@nexora.local>")))
            .multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::plain(body.to_owned()))
                    .singlepart(SinglePart::html(html.to_owned())),
            )
            .map_err(|_| PlatformError::Validation("invalid notification message"))?;
        let mut builder = if config.tls_mode == "tls" {
            SmtpTransport::relay(&config.host)
        } else {
            SmtpTransport::starttls_relay(&config.host)
        }
        .map_err(|_| PlatformError::Validation("invalid SMTP host"))?
        .port(config.port)
        .timeout(Some(Duration::from_secs(20)));
        if !config.username.is_empty() {
            let password = Zeroizing::new(
                String::from_utf8(secret.ok_or(PlatformError::Unavailable)?.to_vec())
                    .map_err(|_| PlatformError::Storage)?,
            );
            builder = builder.credentials(Credentials::new(
                config.username.clone(),
                password.to_string(),
            ));
        }
        builder
            .build()
            .send(&message)
            .map_err(|_| PlatformError::Unavailable)?;
        Ok(())
    }
}
