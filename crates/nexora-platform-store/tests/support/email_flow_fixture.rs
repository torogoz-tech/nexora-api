#[path = "work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::EmailNotification;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
pub struct EmailFlowFixture {
    pub work: WorkItemFixture,
    pub sql: postgres::Client,
}
impl EmailFlowFixture {
    pub fn new() -> Self {
        let mut work = WorkItemFixture::new();
        let password = zeroize::Zeroizing::new(
            std::fs::read_to_string(work.config.secrets_directory.join("postgres_password"))
                .unwrap(),
        );
        let mut sql = postgres::Config::new()
            .host(&work.config.host)
            .port(work.config.postgres_port)
            .user(&work.config.postgres_user)
            .password(password.trim())
            .dbname(&work.config.postgres_database)
            .connect(postgres::NoTls)
            .unwrap();
        sql.batch_execute(&format!("SET search_path TO \"{}\"", work.namespace))
            .unwrap();
        let current = work.store.email_access_settings(&work.ctx).unwrap();
        work.store
            .change_email_access_settings(
                &work.ctx,
                &EmailAccessChange {
                    expected_revision: current.revision,
                    public_web_url: "https://nexora.example".into(),
                },
            )
            .unwrap();
        let smtp = work
            .store
            .installation_service_settings(&work.ctx, InstallationService::Smtp)
            .unwrap();
        work.store.change_installation_service(&work.ctx,InstallationService::Smtp,&InstallationServiceChange{expected_revision:smtp.revision,settings:json!({"enabled":true,"host":"smtp.example.invalid","port":587,"username":"","sender_email":"noreply@example.invalid","sender_name":"Test","tls_mode":"starttls"}),secret:None,remove_secret:false}).unwrap();
        let actor = work.ctx.actor_id().to_owned();
        work.store
            .change_notification_recipient(
                &work.ctx,
                &actor,
                &NotificationRecipientChange {
                    email: "owner@example.invalid".into(),
                    assignments: true,
                    approvals: true,
                    expected_revision: 0,
                },
            )
            .unwrap();
        Self { work, sql }
    }
    pub fn claim_token(&mut self, kind: &str) -> (EmailNotification, zeroize::Zeroizing<String>) {
        let mail = self
            .work
            .store
            .claim_email_notification(true)
            .unwrap()
            .unwrap();
        assert_eq!(mail.kind, kind);
        let (_, body) = self
            .work
            .store
            .email_content_for_delivery(&mail, "Organization")
            .unwrap();
        let token = body
            .split("#token=")
            .nth(1)
            .unwrap()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<String>();
        assert_eq!(token.len(), 68);
        (mail, zeroize::Zeroizing::new(token))
    }
}
