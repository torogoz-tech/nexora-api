use crate::{ApiState, RabbitMqJobWakeupConsumer, smtp_mailer::SmtpMailer};
use nexora_platform::JobKind;
use nexora_platform::{PlatformError, SmtpConfiguration};
use sha2::{Digest, Sha256};
use std::time::Duration;
pub struct EmailNotificationWorker;
impl EmailNotificationWorker {
    pub async fn run(state: ApiState, rabbitmq_uri: String) -> Result<(), PlatformError> {
        let mut retry_seconds = 1;
        let mut startup_recovery_complete = false;
        loop {
            if !startup_recovery_complete {
                if Self::drain_pending(&state).await.is_err() {
                    eprintln!("nexora-notification-worker: PostgreSQL recovery failed");
                    if Self::wait_to_retry(retry_seconds).await {
                        return Ok(());
                    }
                    retry_seconds = (retry_seconds * 2).min(30);
                    continue;
                }
                startup_recovery_complete = true;
            }

            let mut consumer =
                match RabbitMqJobWakeupConsumer::connect(&rabbitmq_uri, JobKind::EmailNotification)
                    .await
                {
                    Ok(consumer) => consumer,
                    Err(_) => {
                        eprintln!("nexora-notification-worker: RabbitMQ unavailable");
                        if Self::wait_to_retry(retry_seconds).await {
                            return Ok(());
                        }
                        retry_seconds = (retry_seconds * 2).min(30);
                        continue;
                    }
                };
            if Self::drain_pending(&state).await.is_err() {
                eprintln!(
                    "nexora-notification-worker: PostgreSQL recovery failed after RabbitMQ connection"
                );
                startup_recovery_complete = false;
                if Self::wait_to_retry(retry_seconds).await {
                    return Ok(());
                }
                retry_seconds = (retry_seconds * 2).min(30);
                continue;
            }
            retry_seconds = 1;

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => return Ok(()),
                    received = consumer.next() => {
                        let wakeup = match received {
                            Ok(Some(wakeup)) => wakeup,
                            Ok(None) | Err(_) => {
                                eprintln!("nexora-notification-worker: RabbitMQ consumer disconnected");
                                break;
                            }
                        };
                        if Self::drain_pending(&state).await.is_ok() {
                            wakeup.ack().await?;
                        } else {
                            wakeup.retry().await?;
                            eprintln!("nexora-notification-worker: PostgreSQL delivery failed; wake-up requeued");
                            break;
                        }
                    }
                }
            }

            if Self::wait_to_retry(retry_seconds).await {
                return Ok(());
            }
            retry_seconds = (retry_seconds * 2).min(30);
        }
    }

    async fn drain_pending(state: &ApiState) -> Result<(), PlatformError> {
        while Self::deliver_next(state).await? {}
        Ok(())
    }

    async fn deliver_next(state: &ApiState) -> Result<bool, PlatformError> {
        let delivery = state
            .run(|store| {
                let Some(runtime) = store.smtp_runtime_settings()? else {
                    return Ok(None);
                };
                let config: SmtpConfiguration = serde_json::from_value(runtime.settings.clone())
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
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        let Some((runtime, config, mail, template, organization, subject, body_html)) = delivery
        else {
            return Ok(false);
        };

        let event_key = mail.event_key.clone();
        let attempts = mail.attempts;
        // Return the database connection and API permit before SMTP network I/O.
        let sent = tokio::task::spawn_blocking(move || {
            let html = template.render_html(&organization, &subject, &body_html);
            let body =
                html2text::from_read(html.as_bytes(), 80).map_err(|_| PlatformError::Storage)?;
            let id = format!("{:x}", Sha256::digest(mail.event_key.as_bytes()));
            SmtpMailer::send(
                &config,
                runtime.secret.as_deref().map(|secret| secret.as_slice()),
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
        state
            .run(move |store| store.finish_email_notification(&event_key, attempts, status))
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        Ok(true)
    }

    async fn wait_to_retry(seconds: u64) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(seconds)) => false,
            _ = tokio::signal::ctrl_c() => true,
        }
    }
}
