use crate::{
    EmailNotification, PlatformStore, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn notification_recipients(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<Vec<NotificationRecipient>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let rows=tx.query("SELECT jsonb_build_object('actor_id',a.id,'name',a.name,'email',COALESCE(r.email,''),'assignments',COALESCE(r.assignments,true),'approvals',COALESCE(r.approvals,true),'revision',COALESCE(r.revision,0)) FROM actors a LEFT JOIN notification_recipients r ON r.actor_id=a.id WHERE a.active AND a.kind='human' ORDER BY a.name,a.id LIMIT 1000",&[]).map_err(Self::database_error)?;
        let data = rows.iter().map(|r| Self::decode(r.get(0))).collect();
        tx.commit().map_err(Self::database_error)?;
        data
    }
    pub fn change_notification_recipient(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &NotificationRecipientChange,
    ) -> Result<NotificationRecipient, PlatformError> {
        if input.email.len() > 320
            || input
                .email
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || !input.email.is_empty() && !email_address::EmailAddress::is_valid(&input.email)
            || input.expected_revision < 0
        {
            return Err(PlatformError::Validation("invalid notification email"));
        }
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "notification.recipient.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let actor_row = tx
            .query_opt(
                "SELECT name FROM actors WHERE id=$1 AND active AND kind='human' FOR UPDATE",
                &[&actor],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let revision = tx
            .query_opt(
                "SELECT revision FROM notification_recipients WHERE actor_id=$1 FOR UPDATE",
                &[&actor],
            )
            .map_err(Self::database_error)?
            .map_or(0, |r| r.get::<_, i64>(0));
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict("notification recipient changed"));
        }
        tx.execute("INSERT INTO notification_recipients(actor_id,email,assignments,approvals) VALUES($1,$2,$3,$4) ON CONFLICT(actor_id) DO UPDATE SET email=EXCLUDED.email,assignments=EXCLUDED.assignments,approvals=EXCLUDED.approvals,revision=notification_recipients.revision+1",&[&actor,&input.email,&input.assignments,&input.approvals]).map_err(Self::database_error)?;
        let value = json!({"actor_id":actor,"name":actor_row.get::<_,String>(0),"email":input.email,"assignments":input.assignments,"approvals":input.approvals,"revision":revision+1});
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "notification.recipient.change",
            Some(actor),
            json!({"revision":revision+1}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
    pub fn queue_smtp_test(
        &mut self,
        ctx: &AuthContext,
        email: &str,
    ) -> Result<Value, PlatformError> {
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "smtp.test",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended('nexora:smtp-tests',0))",
            &[],
        )
        .map_err(Self::database_error)?;
        let pending:i64=tx.query_one("SELECT count(*) FROM email_notifications WHERE kind='test' AND created_at>clock_timestamp()-interval '1 minute'",&[]).map_err(Self::database_error)?.get(0);
        if pending >= 5 {
            return Err(PlatformError::Unavailable);
        }
        let id = crate::SecretGenerator::id("mail")?;
        tx.execute("INSERT INTO email_notifications(event_key,kind,resource_id,actor_id,recipient_override) VALUES($1,'test',$1,$2,$3)",&[&id,&ctx.actor_id(),&email]).map_err(Self::database_error)?;
        let value = json!({"id":id,"status":"pending","attempts":0});
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "smtp.test.queued",
            Some(&id),
            json!({}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn smtp_test_status(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let row=tx.query_opt("SELECT jsonb_build_object('id',event_key,'status',status,'attempts',attempts) FROM email_notifications WHERE event_key=$1 AND kind='test'",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(row.get(0))
    }
    pub fn claim_email_notification(
        &mut self,
        allow_business: bool,
    ) -> Result<Option<EmailNotification>, PlatformError> {
        self.client.execute("UPDATE user_email_changes SET token_secret=NULL WHERE expires_at<=clock_timestamp() OR used_at IS NOT NULL OR attempts>=5",&[]).map_err(Self::database_error)?;
        self.client.execute("UPDATE member_invitations SET token_secret=NULL WHERE expires_at<=clock_timestamp() OR status<>'pending'",&[]).map_err(Self::database_error)?;
        self.client.execute("UPDATE password_reset_requests SET token_secret=NULL WHERE expires_at<=clock_timestamp() OR used_at IS NOT NULL",&[]).map_err(Self::database_error)?;
        self.client.execute("UPDATE email_notifications SET status='failed' WHERE status='delivering' AND attempts>=5 AND available_at<=clock_timestamp()",&[]).map_err(Self::database_error)?;
        let row=self.client.query_opt("UPDATE email_notifications SET status='delivering',attempts=attempts+1,available_at=clock_timestamp()+interval '90 seconds' WHERE event_key=(SELECT event_key FROM email_notifications WHERE status IN ('pending','delivering') AND ($1 OR kind='test') AND attempts<5 AND available_at<=clock_timestamp() ORDER BY available_at,event_key LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING event_key,kind,project_id,resource_id,actor_id,attempts,recipient_override",&[&allow_business]).map_err(Self::database_error)?;
        let Some(row) = row else { return Ok(None) };
        let actor: String = row.get(4);
        let kind: String = row.get(1);
        let project: Option<String> = row.get(2);
        let resource: String = row.get(3);
        let key: String = row.get(0);
        let attempts: i32 = row.get(5);
        if matches!(
            kind.as_str(),
            "invitation"
                | "invitation_accepted"
                | "password_reset"
                | "email_verification"
                | "email_changed"
        ) {
            return self.claim_action_email(&key, &kind, &resource, &actor, attempts);
        }
        if kind == "approval_result" {
            return self.claim_approval_result_email(&key, &resource, &actor, attempts);
        }
        if kind == "test" {
            let authorized = self.client.query_opt("SELECT a.id FROM actors a WHERE a.id=$1 AND a.active AND a.kind='human' AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='installation:manage' AND g.project_id IS NULL)", &[&actor]).map_err(Self::database_error)?.is_some();
            if !authorized {
                self.finish_email_notification(&key, attempts, "skipped")?;
                return Ok(None);
            }
            let email: Option<String> = row.get(6);
            return Ok(Some(EmailNotification {
                event_key: key,
                kind,
                project_id: None,
                resource_id: resource,
                actor_id: actor,
                email: email.ok_or(PlatformError::Storage)?,
                attempts,
                language: self
                    .public_installation_preferences()?
                    .language
                    .as_str()
                    .into(),
            }));
        }
        if kind == "workflow_transition" {
            let recipient=self.client.query_opt("SELECT r.email,(SELECT language FROM installation_preferences WHERE singleton) FROM notification_recipients r JOIN actors a ON a.id=r.actor_id AND a.active AND a.kind='human' JOIN tasks t ON t.id=$3 AND t.project_id IS NOT DISTINCT FROM $2::text AND t.assignee_id=$1 AND NOT t.archived WHERE r.actor_id=$1 AND r.email<>'' AND r.assignments AND ($2::text IS NULL OR EXISTS(SELECT 1 FROM projects p WHERE p.id=$2 AND NOT p.archived)) AND (EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='tasks:read' AND (g.project_id IS NULL OR g.project_id=$2)) OR ($2::text IS NULL AND EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=$1 AND role IN ('super_admin','developer','technical_lead','pmo','manager'))))",&[&actor,&project,&resource]).map_err(Self::database_error)?;
            let Some(recipient) = recipient else {
                self.finish_email_notification(&key, attempts, "skipped")?;
                return Ok(None);
            };
            return Ok(Some(EmailNotification {
                event_key: key,
                kind,
                project_id: project,
                resource_id: resource,
                actor_id: actor,
                email: recipient.get(0),
                attempts,
                language: recipient.get(1),
            }));
        }
        let recipient=self.client.query_opt("SELECT r.email,(SELECT language FROM installation_preferences WHERE singleton) FROM notification_recipients r JOIN actors a ON a.id=r.actor_id AND a.active AND a.kind='human' JOIN projects p ON p.id=$2 AND NOT p.archived WHERE r.actor_id=$1 AND r.email<>'' AND CASE WHEN $3 IN ('assignment','workflow_transition') THEN r.assignments AND EXISTS(SELECT 1 FROM tasks t WHERE t.id=$4 AND t.project_id=$2 AND t.assignee_id=$1 AND NOT t.archived) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='tasks:read' AND (g.project_id IS NULL OR g.project_id=$2)) ELSE r.approvals AND EXISTS(SELECT 1 FROM approval_requests q WHERE q.id=$4 AND q.project_id=$2 AND q.status='pending' AND q.expires_at>clock_timestamp() AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability=CASE WHEN q.target_kind='task' THEN 'tasks:read' ELSE 'documents:read' END AND (g.project_id IS NULL OR g.project_id=$2))) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=$2)) END",&[&actor,&project,&kind,&resource]).map_err(Self::database_error)?;
        let Some(recipient) = recipient else {
            self.finish_email_notification(&key, attempts, "skipped")?;
            return Ok(None);
        };
        Ok(Some(EmailNotification {
            event_key: key,
            kind,
            project_id: project,
            resource_id: resource,
            actor_id: actor,
            email: recipient.get(0),
            attempts,
            language: recipient.get(1),
        }))
    }
    pub fn finish_email_notification(
        &mut self,
        key: &str,
        attempts: i32,
        status: &str,
    ) -> Result<(), PlatformError> {
        if !matches!(status, "sent" | "skipped" | "pending") {
            return Err(PlatformError::Validation("invalid delivery result"));
        }
        self.client.execute("UPDATE email_notifications SET status=CASE WHEN $3='pending' AND attempts>=5 THEN 'failed' ELSE $3 END,delivered_at=CASE WHEN $3='sent' THEN clock_timestamp() ELSE delivered_at END,available_at=clock_timestamp()+LEAST(3600,30*power(2,attempts)::integer)*interval '1 second' WHERE event_key=$1 AND attempts=$2 AND status='delivering'",&[&key,&attempts,&status]).map_err(Self::database_error)?;
        self.client.execute("UPDATE member_invitations SET token_secret=NULL WHERE id IN(SELECT resource_id FROM email_notifications WHERE event_key=$1 AND kind='invitation' AND event_key='invitation:'||member_invitations.id||':'||member_invitations.revision AND status IN ('sent','skipped','failed'))",&[&key]).map_err(Self::database_error)?;
        self.client.execute("UPDATE password_reset_requests SET token_secret=NULL WHERE id IN(SELECT resource_id FROM email_notifications WHERE event_key=$1 AND kind='password_reset' AND status IN ('sent','skipped','failed'))",&[&key]).map_err(Self::database_error)?;
        self.client.execute("UPDATE user_email_changes SET token_secret=NULL WHERE id IN(SELECT resource_id FROM email_notifications WHERE event_key=$1 AND kind='email_verification' AND status IN ('sent','skipped','failed'))",&[&key]).map_err(Self::database_error)?;
        Ok(())
    }
}
