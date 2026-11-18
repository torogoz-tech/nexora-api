use crate::{
    PlatformStore, ProviderSecretContext, ProviderSecretEnvelope, SecretAuthority, SecretGenerator,
    audit_writer::AuditWriter, login_gate::LoginGate, totp_code::TotpCode,
};
use nexora_platform::{AuthContext, AuthMethod, Capability, PlatformError};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
impl PlatformStore {
    /// Only first-factor challenge credentials are accepted here, never as ordinary API sessions.
    pub fn identity_gate(
        &mut self,
        token: &str,
        action: &str,
        code: &str,
    ) -> Result<Value, PlatformError> {
        if token.len() > 256
            || code.len() > 6
            || ![
                "status",
                "setup",
                "verify",
                "skip",
                "choose_totp",
                "choose_passkey",
            ]
            .contains(&action)
        {
            return Err(PlatformError::Unauthorized);
        }
        let (id, _) = token
            .strip_prefix("nxs_")
            .and_then(|s| s.split_once('.'))
            .ok_or(PlatformError::Unauthorized)?;
        let cipher = self.provider_secret_cipher()?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT s.actor_id,s.digest,s.access_stage,s.setup_secret FROM sessions s JOIN actors a ON a.id=s.actor_id WHERE s.id=$1 AND s.revoked_at IS NULL AND s.expires_at>now() AND s.created_at>now()-interval '15 minutes' AND a.active AND (s.authentication_provider_id IS NULL OR EXISTS(SELECT 1 FROM oidc_providers p WHERE p.id=s.authentication_provider_id AND p.enabled)) FOR UPDATE OF s",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let digest: Vec<u8> = row.get(1);
        if !bool::from(digest.ct_eq(&SecretGenerator::digest(token)))
            || row.get::<_, String>(2) == "ready"
        {
            return Err(PlatformError::Unauthorized);
        }
        let actor: String = row.get(0);
        let stage = LoginGate::apply(&mut tx, &actor, id)?.unwrap_or_else(|| "ready".into());
        let security=tx.query_one("SELECT skipped,secret,last_step,COALESCE(blocked_until>now(),false),extract(epoch FROM now())::bigint FROM user_security WHERE actor_id=$1 FOR UPDATE",&[&actor]).map_err(Self::database_error)?;
        if security.get::<_, bool>(3) {
            return Err(PlatformError::Conflict(
                "too many verification attempts; wait five minutes",
            ));
        }
        let skipped: i32 = security.get(0);
        let context = ProviderSecretContext {
            binding_id: actor.clone(),
            provider: SecretAuthority::Totp,
            host: "nexora.local".into(),
            purpose: "totp".into(),
            revision: 1,
        };
        let mut result = json!({"stage":stage,"remaining_skips":5-skipped});
        let mut complete = stage == "ready";
        match action {
            "skip" if (stage == "mfa_select" || stage == "totp_enroll") && skipped < 5 => {
                tx.execute(
                    "UPDATE user_security SET skipped=skipped+1 WHERE actor_id=$1",
                    &[&actor],
                )
                .map_err(Self::database_error)?;
                complete = true;
            }
            "choose_totp" if stage == "mfa_select" => {
                tx.execute(
                    "UPDATE user_security SET primary_auth_method='totp' WHERE actor_id=$1",
                    &[&actor],
                )
                .map_err(Self::database_error)?;
                let has_secret = security.get::<_, Option<Value>>(1).is_some();
                result["stage"] = json!(if has_secret {
                    "totp_verify"
                } else {
                    "totp_enroll"
                });
            }
            "choose_passkey" if stage == "mfa_select" => {
                let has_passkey: bool = tx
                    .query_one(
                        "SELECT EXISTS(SELECT 1 FROM passkey_credentials WHERE actor_id=$1)",
                        &[&actor],
                    )
                    .map_err(Self::database_error)?
                    .get(0);
                if !has_passkey {
                    return Err(PlatformError::Validation(
                        "security method is not configured",
                    ));
                }
                tx.execute(
                    "UPDATE user_security SET primary_auth_method='passkey' WHERE actor_id=$1",
                    &[&actor],
                )
                .map_err(Self::database_error)?;
                result["stage"] = json!("passkey_verify");
            }
            "setup" if stage == "totp_enroll" => {
                let sealed = if let Some(value) = row.get::<_, Option<Value>>(3) {
                    value
                } else {
                    let mut seed = zeroize::Zeroizing::new(vec![0u8; 20]);
                    getrandom::fill(&mut seed).map_err(|_| PlatformError::Unavailable)?;
                    let value = serde_json::to_value(cipher.seal(&context, &seed)?)
                        .map_err(|_| PlatformError::Storage)?;
                    tx.execute(
                        "UPDATE sessions SET setup_secret=$2 WHERE id=$1",
                        &[&id, &value],
                    )
                    .map_err(Self::database_error)?;
                    value
                };
                let envelope: ProviderSecretEnvelope = Self::decode(sealed)?;
                let seed = cipher.open(&context, &envelope)?;
                let secret = TotpCode::encode(&seed);
                let account = tx
                    .query_one("SELECT username FROM users WHERE actor_id=$1", &[&actor])
                    .map_err(Self::database_error)?
                    .get::<_, String>(0);
                let label: String =
                    url::form_urlencoded::byte_serialize(format!("Nexora:{account}").as_bytes())
                        .collect();
                result["secret"] = json!(secret);
                result["uri"] = json!(format!(
                    "otpauth://totp/{label}?secret={secret}&issuer=Nexora&algorithm=SHA1&digits=6&period=30"
                ));
            }
            "verify" if stage == "totp_enroll" || stage == "totp_verify" => {
                let sealed: Value = if stage == "totp_enroll" {
                    row.get::<_, Option<Value>>(3)
                } else {
                    security.get::<_, Option<Value>>(1)
                }
                .ok_or(PlatformError::Validation("start TOTP setup first"))?;
                let envelope: ProviderSecretEnvelope = Self::decode(sealed.clone())?;
                let seed = cipher.open(&context, &envelope)?;
                let step = TotpCode::verify(&seed, code, security.get(4), security.get(2));
                let Some(step) = step else {
                    tx.execute("UPDATE user_security SET failed_attempts=CASE WHEN failed_attempts>=4 THEN 0 ELSE failed_attempts+1 END,blocked_until=CASE WHEN failed_attempts>=4 THEN now()+interval '5 minutes' ELSE blocked_until END WHERE actor_id=$1",&[&actor]).map_err(Self::database_error)?;
                    tx.commit().map_err(Self::database_error)?;
                    return Err(PlatformError::Unauthorized);
                };
                tx.execute("UPDATE user_security SET secret=$2,last_step=$3,failed_attempts=0,blocked_until=NULL WHERE actor_id=$1",&[&actor,&sealed,&step]).map_err(Self::database_error)?;
                tx.execute("UPDATE user_security SET primary_auth_method='totp' WHERE actor_id=$1 AND primary_auth_method='none'", &[&actor]).map_err(Self::database_error)?;
                complete = true;
                // Another challenge cannot overwrite the newly enrolled seed.
                tx.execute(
                    "UPDATE sessions SET setup_secret=NULL WHERE actor_id=$1 AND id<>$2",
                    &[&actor, &id],
                )
                .map_err(Self::database_error)?;
            }
            "status" => {}
            _ => return Err(PlatformError::Forbidden),
        }
        if complete {
            let new_token = format!("nxs_{id}.{}", SecretGenerator::token("secret")?);
            tx.execute("UPDATE sessions SET access_stage='ready',setup_secret=NULL,digest=$2,expires_at=LEAST(clock_timestamp()+interval '30 minutes',absolute_expires_at),last_activity_at=clock_timestamp() WHERE id=$1",&[&id,&SecretGenerator::digest(&new_token)]).map_err(Self::database_error)?;
            result = json!({"stage":"ready","access_token":new_token,"token_type":"Bearer","expires_in":28800,"actor_id":actor});
        }
        if action != "status" || complete {
            AuditWriter::append(
                &mut tx,
                Some(&actor),
                Some(id),
                "identity.gate_action",
                Some(&actor),
                json!({"action":action,"completed":complete}),
            )?;
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn member_totp(&mut self, ctx: &AuthContext, actor: &str) -> Result<Value, PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.actor_id() != actor {
            ctx.require(Capability::ActorsManage, None)?;
        }
        let row=self.client.query_opt("SELECT COALESCE(s.totp_exempt,false),s.secret IS NOT NULL,COALESCE(s.skipped,0),COALESCE(s.primary_auth_method,'none') FROM users u LEFT JOIN user_security s ON s.actor_id=u.actor_id WHERE u.actor_id=$1",&[&actor]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        Ok(
            json!({"exempt":row.get::<_,bool>(0),"enrolled":row.get::<_,bool>(1),"remaining_skips":5-row.get::<_,i32>(2),"active_method":row.get::<_,String>(3)}),
        )
    }
    pub fn set_totp_exemption(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        exempt: bool,
    ) -> Result<(), PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let authorized = tx
            .query_opt(
                "SELECT role FROM actor_roles WHERE actor_id=$1 AND role='super_admin' FOR SHARE",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?;
        if authorized.is_none() {
            return Err(PlatformError::Forbidden);
        }
        tx.execute("INSERT INTO user_security(actor_id,totp_exempt) VALUES($1,$2) ON CONFLICT(actor_id) DO UPDATE SET totp_exempt=EXCLUDED.totp_exempt",&[&actor,&exempt]).map_err(Self::database_error)?;
        // Existing sessions must reauthenticate when the exemption changes.
        tx.execute(
            "UPDATE sessions SET revoked_at=now() WHERE actor_id=$1 AND revoked_at IS NULL",
            &[&actor],
        )
        .map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "identity.totp_exemption",
            Some(actor),
            json!({"exempt":exempt}),
        )?;
        tx.commit().map_err(Self::database_error)
    }
}
