use crate::{ApiState, smtp_mailer::SmtpMailer};
use nexora_platform::{PlatformError, SmtpConfiguration};
use sha2::{Digest, Sha256};
pub struct EmailNotificationWorker;
impl EmailNotificationWorker {
    pub fn start(state: ApiState) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let delivery = state
                    .run(|store| {
                        let Some(runtime) = store.smtp_runtime_settings()? else {
                            return Ok(None);
                        };
                        let config: SmtpConfiguration =
                            serde_json::from_value(runtime.settings.clone())
                                .map_err(|_| PlatformError::Storage)?;
                        let Some(mail) = store.claim_email_notification(config.enabled)? else {
                            return Ok(None);
                        };
                        let (template, organization) = store.email_template_for_delivery()?;
                        let (subject, body_html) =
                            store.email_content_for_delivery(&mail, &organization)?;
                        Ok(Some((
                            runtime,
                            config,
                            mail,
                            template,
                            organization,
                            subject,
                            body_html,
                        )))
                    })
                    .await;
                if let Ok(Some((
                    runtime,
                    config,
                    mail,
                    template,
                    organization,
                    subject,
                    body_html,
                ))) = delivery
                {
                    let event_key = mail.event_key.clone();
                    let attempts = mail.attempts;
                    // Return the database connection and API permit before SMTP network I/O.
                    let sent = tokio::task::spawn_blocking(move || {
                        let html = template.render_html(&organization, &subject, &body_html);
                        let body = html2text::from_read(html.as_bytes(), 80)
                            .map_err(|_| PlatformError::Storage)?;
                        let id = format!("{:x}", Sha256::digest(mail.event_key.as_bytes()));
                        SmtpMailer::send(
                            &config,
                            runtime.secret.as_deref().map(|s| s.as_slice()),
                            &mail.email,
                            &subject,
                            &body,
                            &id,
                            &html,
                        )
                    })
                    .await;
                    let status = if matches!(sent, Ok(Ok(()))) {
                        "sent"
                    } else {
                        "pending"
                    };
                    let _ = state
                        .run(move |store| {
                            store.finish_email_notification(&event_key, attempts, status)
                        })
                        .await;
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        })
    }
}
