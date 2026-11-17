use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    password_credential::PasswordCredential, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
use zeroize::Zeroizing;
impl PlatformStore {
    pub fn member_requests(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)?;
        let mut requests = tx.query("SELECT jsonb_build_object('id',a.id,'kind','identity','email',COALESCE(i.contact_email,u.username),'name',a.name,'created_at',a.created_at,'role',NULL) FROM actors a JOIN users u ON u.actor_id=a.id JOIN user_security s ON s.actor_id=a.id LEFT JOIN oidc_identities i ON i.actor_id=a.id WHERE a.kind='human' AND a.active AND s.pending_role AND NOT EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=a.id) ORDER BY a.created_at DESC,a.id", &[]).map_err(Self::database_error)?.into_iter().map(|row| row.get(0)).collect::<Vec<Value>>();
        requests.extend(tx.query("SELECT jsonb_build_object('id',id,'kind','invitation','email',email,'name',name,'created_at',created_at,'expires_at',expires_at,'status',status) FROM member_invitations WHERE status='pending' AND expires_at>clock_timestamp() ORDER BY created_at DESC,id", &[]).map_err(Self::database_error)?.into_iter().map(|row| row.get(0)));
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items": requests}))
    }
    pub fn member_invitations(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<Vec<MemberInvitation>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)?;
        let rows=tx.query("SELECT jsonb_build_object('id',id,'email',email,'name',name,'status',CASE WHEN status='pending' AND expires_at<=clock_timestamp() THEN 'expired' ELSE status END,'revision',revision,'expires_at',expires_at,'created_at',created_at) FROM member_invitations ORDER BY created_at DESC,id LIMIT 1000",&[]).map_err(Self::database_error)?;
        let data = rows.iter().map(|r| Self::decode(r.get(0))).collect();
        tx.commit().map_err(Self::database_error)?;
        data
    }
    pub fn create_member_invitation(
        &mut self,
        ctx: &AuthContext,
        input: &MemberInvitationCreate,
    ) -> Result<MemberInvitation, PlatformError> {
        if ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        let email = input.email.trim().to_ascii_lowercase();
        let name = DomainText::name(&input.name)?;
        if email.len() > 320
            || !email_address::EmailAddress::is_valid(&email)
            || email.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(PlatformError::Validation("invalid invitation email"));
        }
        let cipher = self.email_token_cipher()?;
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "member.invite",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(v);
        }
        let web: String = tx
            .query_one(
                "SELECT public_web_url FROM email_access_settings WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        Self::require_invitation_delivery(&mut tx, &web)?;
        if tx
            .query_opt(
                "SELECT actor_id FROM notification_recipients WHERE lower(email)=$1 LIMIT 1",
                &[&email],
            )
            .map_err(Self::database_error)?
            .is_some()
        {
            return Err(PlatformError::Conflict("email already belongs to a member"));
        }
        let id = SecretGenerator::id("inv")?;
        let token = Zeroizing::new(SecretGenerator::token("nxi")?);
        let secret = cipher.seal(&id, 1, &token)?;
        let digest = SecretGenerator::digest(&token);
        tx.execute("INSERT INTO member_invitations(id,email,name,invited_by,digest,token_secret,public_web_url,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,clock_timestamp()+interval '72 hours')",&[&id,&email,&name,&ctx.actor_id(),&digest,&secret,&web]).map_err(Self::database_error)?;
        tx.execute("INSERT INTO email_notifications(event_key,kind,resource_id,actor_id,recipient_override) VALUES($1,'invitation',$2,$3,$4)",&[&format!("invitation:{id}:1"),&id,&ctx.actor_id(),&email]).map_err(Self::database_error)?;
        let value:Value=tx.query_one("SELECT jsonb_build_object('id',id,'email',email,'name',name,'status',status,'revision',revision,'expires_at',expires_at,'created_at',created_at) FROM member_invitations WHERE id=$1",&[&id]).map_err(Self::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "member.invited",
            Some(&id),
            json!({}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
    fn require_invitation_delivery(
        tx: &mut postgres::Transaction<'_>,
        web: &str,
    ) -> Result<(), PlatformError> {
        let valid:bool=tx.query_one("SELECT (SELECT local_enabled FROM authentication_policy WHERE singleton) AND EXISTS(SELECT 1 FROM installation_services WHERE kind='smtp' AND (settings->>'enabled')::boolean)",&[]).map_err(Self::database_error)?.get(0);
        if web.is_empty() || !valid {
            return Err(PlatformError::Validation(
                "configure public web URL, enabled SMTP and local authentication before inviting members",
            ));
        }
        Ok(())
    }
    pub fn change_member_invitation(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &MemberInvitationChange,
        resend: bool,
    ) -> Result<MemberInvitation, PlatformError> {
        if resend && ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        let cipher = self.email_token_cipher()?;
        let receipt = self.request_context.clone();
        let operation = if resend {
            "member.invitation.resend"
        } else {
            "member.invitation.revoke"
        };
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let row = tx
            .query_opt(
                "SELECT email,revision,status FROM member_invitations WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let revision: i64 = row.get(1);
        let status: String = row.get(2);
        if revision != input.expected_revision || status != "pending" {
            return Err(PlatformError::Conflict("invitation already changed"));
        }
        if resend {
            let web: String = tx
                .query_one(
                    "SELECT public_web_url FROM email_access_settings WHERE singleton",
                    &[],
                )
                .map_err(Self::database_error)?
                .get(0);
            Self::require_invitation_delivery(&mut tx, &web)?;
            let token = Zeroizing::new(SecretGenerator::token("nxi")?);
            let secret = cipher.seal(id, revision + 1, &token)?;
            let digest = SecretGenerator::digest(&token);
            tx.execute("UPDATE member_invitations SET digest=$2,token_secret=$3,revision=revision+1,public_web_url=$4,invited_by=$5,expires_at=clock_timestamp()+interval '72 hours' WHERE id=$1",&[&id,&digest,&secret,&web,&ctx.actor_id()]).map_err(Self::database_error)?;
            tx.execute("INSERT INTO email_notifications(event_key,kind,resource_id,actor_id,recipient_override) VALUES($1,'invitation',$2,$3,$4)",&[&format!("invitation:{id}:{}",revision+1),&id,&ctx.actor_id(),&row.get::<_,String>(0)]).map_err(Self::database_error)?;
        } else {
            tx.execute("UPDATE member_invitations SET status='revoked',token_secret=NULL,revision=revision+1 WHERE id=$1",&[&id]).map_err(Self::database_error)?;
        }
        let value:Value=tx.query_one("SELECT jsonb_build_object('id',id,'email',email,'name',name,'status',status,'revision',revision,'expires_at',expires_at,'created_at',created_at) FROM member_invitations WHERE id=$1",&[&id]).map_err(Self::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            Some(id),
            json!({"revision":revision+1}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Self::decode(value)
    }
    pub fn inspect_member_invitation(&mut self, token: &str) -> Result<Value, PlatformError> {
        if !Self::valid_email_action_token(token, "nxi_") {
            return Err(PlatformError::Validation("invalid or expired invitation"));
        }
        self.client.query_opt("SELECT jsonb_build_object('name',i.name,'email',i.email,'organization_name',p.name,'expires_at',i.expires_at) FROM member_invitations i CROSS JOIN organization_profile p WHERE i.digest=$1 AND i.status='pending' AND i.expires_at>clock_timestamp() AND i.public_web_url=(SELECT public_web_url FROM email_access_settings WHERE singleton) AND (SELECT local_enabled FROM authentication_policy WHERE singleton) AND EXISTS(SELECT 1 FROM actors a JOIN actor_grants g ON g.actor_id=a.id WHERE a.id=i.invited_by AND a.active AND a.kind='human' AND g.capability='actors:manage' AND g.project_id IS NULL)",&[&SecretGenerator::digest(token)]).map_err(Self::database_error)?.map(|r|r.get(0)).ok_or(PlatformError::Validation("invalid or expired invitation"))
    }
    pub(crate) fn valid_email_action_token(token: &str, prefix: &str) -> bool {
        token.starts_with(prefix)
            && token.len() == prefix.len() + 64
            && token[prefix.len()..].bytes().all(|b| b.is_ascii_hexdigit())
    }
    pub fn accept_member_invitation(
        &mut self,
        input: &MemberInvitationAccept,
    ) -> Result<(), PlatformError> {
        if !Self::valid_email_action_token(&input.token, "nxi_") {
            return Err(PlatformError::Validation("invalid or expired invitation"));
        }
        let username = DomainText::username(&input.username)?;
        let name = DomainText::name(&input.name)?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT id,email,invited_by FROM member_invitations WHERE digest=$1 AND status='pending' AND expires_at>clock_timestamp() AND public_web_url=(SELECT public_web_url FROM email_access_settings WHERE singleton) FOR UPDATE",&[&SecretGenerator::digest(&input.token)]).map_err(Self::database_error)?.ok_or(PlatformError::Validation("invalid or expired invitation"))?;
        let id: String = row.get(0);
        let email: String = row.get(1);
        let inviter: String = row.get(2);
        let allowed:bool=tx.query_one("SELECT (SELECT local_enabled FROM authentication_policy WHERE singleton) AND EXISTS(SELECT 1 FROM actors a JOIN actor_grants g ON g.actor_id=a.id WHERE a.id=$1 AND a.active AND a.kind='human' AND g.capability='actors:manage' AND g.project_id IS NULL)",&[&inviter]).map_err(Self::database_error)?.get(0);
        if !allowed {
            return Err(PlatformError::Validation("invalid or expired invitation"));
        }
        if tx
            .query_opt(
                "SELECT actor_id FROM notification_recipients WHERE lower(email)=$1",
                &[&email],
            )
            .map_err(Self::database_error)?
            .is_some()
        {
            return Err(PlatformError::Conflict("email already belongs to a member"));
        }
        let inviter_authorized = tx.query_opt("SELECT a.id FROM actors a JOIN actor_grants g ON g.actor_id=a.id WHERE a.id=$1 AND a.active AND a.kind='human' AND g.capability='actors:manage' AND g.project_id IS NULL FOR SHARE OF a,g", &[&inviter]).map_err(Self::database_error)?.is_some();
        let local_enabled: bool = tx
            .query_one(
                "SELECT local_enabled FROM authentication_policy WHERE singleton FOR SHARE",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if !inviter_authorized || !local_enabled {
            return Err(PlatformError::Validation("invalid or expired invitation"));
        }
        let hash = PasswordCredential::hash(&input.password)?;
        let actor = Actor::new(
            SecretGenerator::id("act")?,
            ActorKind::Human,
            &name,
            "Invited member",
        )?;
        tx.execute(
            "INSERT INTO actors(id,kind,name,purpose) VALUES($1,'human',$2,'Invited member')",
            &[&actor.id, &name],
        )
        .map_err(Self::database_error)?;
        tx.execute(
            "INSERT INTO users(actor_id,username,password_hash) VALUES($1,$2,$3)",
            &[&actor.id, &username, &hash],
        )
        .map_err(Self::database_error)?;
        tx.execute(
            "INSERT INTO notification_recipients(actor_id,email) VALUES($1,$2)",
            &[&actor.id, &email],
        )
        .map_err(Self::database_error)?;
        tx.execute(
            "INSERT INTO user_security(actor_id,pending_role) VALUES($1,true)",
            &[&actor.id],
        )
        .map_err(Self::database_error)?;
        tx.execute("UPDATE member_invitations SET status='accepted',accepted_by=$2,accepted_at=clock_timestamp(),token_secret=NULL,revision=revision+1 WHERE id=$1",&[&id,&actor.id]).map_err(Self::database_error)?;
        tx.execute("INSERT INTO email_notifications(event_key,kind,resource_id,actor_id) VALUES($1,'invitation_accepted',$2,$3) ON CONFLICT DO NOTHING",&[&format!("invitation_accepted:{id}"),&id,&inviter]).map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(&actor.id),
            None,
            "member.invitation.accepted",
            Some(&id),
            json!({"invited_by":inviter}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(())
    }
}
