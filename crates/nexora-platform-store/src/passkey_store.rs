use crate::{PlatformStore, SecretGenerator, audit_writer::AuditWriter};
use nexora_platform::{AuthContext, AuthMethod, IssuedSession, PlatformError};
use passkey_auth::{
    AuthenticationResponse, AuthenticationState, CosePublicKey, CredentialId, PasskeyCredential,
    RegistrationResponse, RegistrationState, Webauthn,
};
use serde_json::{Value, json};

const CEREMONY_TTL_SECONDS: i64 = 300;

fn ceremony_error() -> PlatformError {
    PlatformError::Validation("invalid or expired passkey ceremony")
}

fn webauthn(rp_id: &str, origin: &str) -> Webauthn {
    Webauthn::new(rp_id, "Nexora", origin)
        .require_user_verification(true)
        .strict_base64(true)
}

impl PlatformStore {
    pub fn start_passkey_gate_authentication(
        &mut self,
        token: &str,
        rp_id: &str,
        origin: &str,
    ) -> Result<Value, PlatformError> {
        let (session_id, _) = token
            .strip_prefix("nxs_")
            .and_then(|value| value.split_once('.'))
            .ok_or(PlatformError::Unauthorized)?;
        let digest = SecretGenerator::digest(token);
        let actor: String = self.client.query_opt(
            "SELECT actor_id FROM sessions WHERE id=$1 AND digest=$2 AND access_stage='passkey_verify' AND revoked_at IS NULL AND expires_at>now()",
            &[&session_id, &digest],
        ).map_err(Self::database_error)?.ok_or(PlatformError::Unauthorized)?.get(0);
        let rows = self.client.query("SELECT credential_id,public_key_cose,sign_count FROM passkey_credentials WHERE actor_id=$1 ORDER BY id", &[&actor]).map_err(Self::database_error)?;
        if rows.is_empty() {
            return Err(PlatformError::Validation(
                "security method is not configured",
            ));
        }
        let credentials = rows
            .into_iter()
            .map(|row| PasskeyCredential {
                id: CredentialId(row.get(0)),
                public_key_cose: CosePublicKey(row.get(1)),
                counter: row.get::<_, i64>(2).try_into().unwrap_or(u32::MAX),
                transports: Vec::new(),
                aaguid: [0; 16],
            })
            .collect::<Vec<_>>();
        let (challenge, state) = webauthn(rp_id, origin)
            .start_authentication_with_creds_for_user(actor.as_bytes(), &credentials);
        let ceremony_id = format!("pca_{}", SecretGenerator::token("ceremony")?);
        let state = serde_json::to_value(state).map_err(|_| PlatformError::Storage)?;
        self.client.execute("INSERT INTO passkey_ceremonies(id,actor_id,kind,state,expires_at) VALUES($1,$2,'authentication',$3,clock_timestamp()+$4::bigint*interval '1 second')", &[&ceremony_id, &actor, &state, &CEREMONY_TTL_SECONDS]).map_err(Self::database_error)?;
        Ok(json!({"ceremony_id": ceremony_id, "challenge": challenge}))
    }

    pub fn finish_passkey_gate_authentication(
        &mut self,
        token: &str,
        ceremony_id: &str,
        response: Value,
        rp_id: &str,
        origin: &str,
    ) -> Result<IssuedSession, PlatformError> {
        let (session_id, _) = token
            .strip_prefix("nxs_")
            .and_then(|value| value.split_once('.'))
            .ok_or(PlatformError::Unauthorized)?;
        let digest = SecretGenerator::digest(token);
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let session = tx.query_opt("SELECT actor_id FROM sessions WHERE id=$1 AND digest=$2 AND access_stage='passkey_verify' AND revoked_at IS NULL AND expires_at>now() FOR UPDATE", &[&session_id, &digest]).map_err(Self::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let actor: String = session.get(0);
        let row = tx.query_opt("SELECT state FROM passkey_ceremonies WHERE id=$1 AND actor_id=$2 AND kind='authentication' AND consumed_at IS NULL AND expires_at>clock_timestamp() FOR UPDATE", &[&ceremony_id, &actor]).map_err(Self::database_error)?.ok_or_else(ceremony_error)?;
        let state: AuthenticationState =
            serde_json::from_value(row.get(0)).map_err(|_| ceremony_error())?;
        let response: AuthenticationResponse =
            serde_json::from_value(response).map_err(|_| ceremony_error())?;
        let credential_id =
            CredentialId::from_b64url(&response.id).map_err(|_| ceremony_error())?;
        let credential_row = tx.query_opt("SELECT public_key_cose,sign_count FROM passkey_credentials WHERE actor_id=$1 AND credential_id=$2 FOR UPDATE", &[&actor, &credential_id.0]).map_err(Self::database_error)?.ok_or_else(ceremony_error)?;
        let credential = PasskeyCredential {
            id: credential_id,
            public_key_cose: CosePublicKey(credential_row.get(0)),
            counter: credential_row
                .get::<_, i64>(1)
                .try_into()
                .unwrap_or(u32::MAX),
            transports: Vec::new(),
            aaguid: [0; 16],
        };
        let success = webauthn(rp_id, origin)
            .finish_authentication(&state, &response, &credential)
            .map_err(|_| ceremony_error())?;
        let new_token = format!("nxs_{session_id}.{}", SecretGenerator::token("secret")?);
        tx.execute("UPDATE passkey_credentials SET sign_count=$3,last_used_at=clock_timestamp() WHERE actor_id=$1 AND credential_id=$2", &[&actor, &success.credential_id.0, &i64::from(success.new_counter)]).map_err(Self::database_error)?;
        tx.execute(
            "UPDATE passkey_ceremonies SET consumed_at=clock_timestamp() WHERE id=$1",
            &[&ceremony_id],
        )
        .map_err(Self::database_error)?;
        tx.execute("UPDATE sessions SET access_stage='ready',digest=$2,expires_at=now()+interval '8 hours' WHERE id=$1", &[&session_id, &SecretGenerator::digest(&new_token)]).map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(&actor),
            Some(&session_id),
            "identity.gate_passkey",
            Some(&actor),
            json!({"method":"passkey"}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(IssuedSession {
            access_token: new_token,
            token_type: "Bearer".into(),
            expires_in: 28800,
            actor_id: actor,
            next_step: None,
        })
    }
    pub fn passkey_status(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let ctx = self.fresh(ctx)?;
        let rows = self.client.query(
            "SELECT id,label,created_at::text,last_used_at::text FROM passkey_credentials WHERE actor_id=$1 ORDER BY created_at,id",
            &[&ctx.actor_id()],
        ).map_err(Self::database_error)?;
        let credentials = rows
            .into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<_, String>(0),
                    "label": row.get::<_, String>(1),
                    "created_at": row.get::<_, String>(2),
                    "last_used_at": row.get::<_, Option<String>>(3),
                })
            })
            .collect::<Vec<_>>();
        let active_method = self
            .client
            .query_opt(
                "SELECT primary_auth_method FROM user_security WHERE actor_id=$1",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .and_then(|row| row.get::<_, Option<String>>(0));
        Ok(
            json!({"enabled": !credentials.is_empty(), "credentials": credentials, "active_method": active_method.unwrap_or_else(|| "none".into())}),
        )
    }

    pub fn set_primary_auth_method(
        &mut self,
        ctx: &AuthContext,
        method: &str,
    ) -> Result<Value, PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session || !["none", "totp", "passkey"].contains(&method) {
            return Err(PlatformError::Forbidden);
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let available = tx.query_one(
            "SELECT EXISTS(SELECT 1 FROM user_security WHERE actor_id=$1 AND secret IS NOT NULL), EXISTS(SELECT 1 FROM passkey_credentials WHERE actor_id=$1)",
            &[&ctx.actor_id()],
        ).map_err(Self::database_error)?;
        let has_totp: bool = available.get(0);
        let has_passkey: bool = available.get(1);
        if (method == "totp" && !has_totp) || (method == "passkey" && !has_passkey) {
            return Err(PlatformError::Validation(
                "security method is not configured",
            ));
        }
        tx.execute(
            "INSERT INTO user_security(actor_id,primary_auth_method) VALUES($1,$2) ON CONFLICT(actor_id) DO UPDATE SET primary_auth_method=EXCLUDED.primary_auth_method",
            &[&ctx.actor_id(), &method],
        ).map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "identity.primary_auth_method_changed",
            Some(ctx.actor_id()),
            json!({"method": method}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"active_method": method, "totp": has_totp, "passkey": has_passkey}))
    }

    pub fn start_passkey_registration(
        &mut self,
        ctx: &AuthContext,
        rp_id: &str,
        origin: &str,
    ) -> Result<Value, PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session || rp_id.is_empty() || origin.is_empty() {
            return Err(PlatformError::Forbidden);
        }
        let user = self.client.query_opt(
            "SELECT u.username,a.name FROM users u JOIN actors a ON a.id=u.actor_id WHERE u.actor_id=$1 AND a.active",
            &[&ctx.actor_id()],
        ).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        let existing = self
            .client
            .query(
                "SELECT credential_id FROM passkey_credentials WHERE actor_id=$1",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .into_iter()
            .map(|row| CredentialId(row.get(0)))
            .collect::<Vec<_>>();
        let (challenge, state) = webauthn(rp_id, origin).start_registration(
            ctx.actor_id().as_bytes(),
            user.get::<_, String>(0).as_str(),
            user.get::<_, String>(1).as_str(),
            &existing,
        );
        let ceremony_id = format!("pcr_{}", SecretGenerator::token("ceremony")?);
        let state = serde_json::to_value(state).map_err(|_| PlatformError::Storage)?;
        self.client.execute(
            "INSERT INTO passkey_ceremonies(id,actor_id,kind,state,expires_at) VALUES($1,$2,'registration',$3,clock_timestamp()+$4::bigint*interval '1 second')",
            &[&ceremony_id, &ctx.actor_id(), &state, &CEREMONY_TTL_SECONDS],
        ).map_err(Self::database_error)?;
        Ok(json!({"ceremony_id": ceremony_id, "challenge": challenge}))
    }

    pub fn finish_passkey_registration(
        &mut self,
        ctx: &AuthContext,
        ceremony_id: &str,
        response: Value,
        label: Option<&str>,
        rp_id: &str,
        origin: &str,
    ) -> Result<Value, PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session || !ceremony_id.starts_with("pcr_") {
            return Err(PlatformError::Forbidden);
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row = tx.query_opt(
            "SELECT state FROM passkey_ceremonies WHERE id=$1 AND actor_id=$2 AND kind='registration' AND consumed_at IS NULL AND expires_at>clock_timestamp() FOR UPDATE",
            &[&ceremony_id, &ctx.actor_id()],
        ).map_err(Self::database_error)?.ok_or_else(ceremony_error)?;
        let state: RegistrationState =
            serde_json::from_value(row.get(0)).map_err(|_| ceremony_error())?;
        let response: RegistrationResponse =
            serde_json::from_value(response).map_err(|_| ceremony_error())?;
        let credential = webauthn(rp_id, origin)
            .finish_registration(&state, &response)
            .map_err(|_| ceremony_error())?;
        let credential_id = credential.id.0.clone();
        let id = format!("pk_{}", SecretGenerator::token("credential")?);
        let label = label
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Passkey")
            .chars()
            .take(120)
            .collect::<String>();
        tx.execute(
            "INSERT INTO passkey_credentials(id,actor_id,credential_id,public_key_cose,sign_count,label) VALUES($1,$2,$3,$4,$5,$6)",
            &[&id, &ctx.actor_id(), &credential_id, &credential.public_key_cose.0, &(i64::from(credential.counter)), &label],
        ).map_err(|error| if error.code().map(|code| code.code()) == Some("23505") { ceremony_error() } else { Self::database_error(error) })?;
        tx.execute(
            "UPDATE passkey_ceremonies SET consumed_at=clock_timestamp() WHERE id=$1",
            &[&ceremony_id],
        )
        .map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "identity.passkey_registered",
            Some(&id),
            json!({"label":label}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"id": id, "label": label, "enabled": true}))
    }

    pub fn start_passkey_authentication(
        &mut self,
        username: &str,
        rp_id: &str,
        origin: &str,
    ) -> Result<Value, PlatformError> {
        if username.trim().is_empty() || rp_id.is_empty() || origin.is_empty() {
            return Err(PlatformError::Unauthorized);
        }
        let user = self.client.query_opt(
            "SELECT u.actor_id,a.name FROM users u JOIN actors a ON a.id=u.actor_id WHERE u.username=$1 AND a.active AND a.kind='human'",
            &[&username.trim()],
        ).map_err(Self::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let actor: String = user.get(0);
        let rows = self.client.query(
            "SELECT credential_id,public_key_cose,sign_count FROM passkey_credentials WHERE actor_id=$1 ORDER BY id",
            &[&actor],
        ).map_err(Self::database_error)?;
        if rows.is_empty() {
            return Err(PlatformError::Unauthorized);
        }
        let credentials = rows
            .into_iter()
            .map(|row| PasskeyCredential {
                id: CredentialId(row.get(0)),
                public_key_cose: CosePublicKey(row.get(1)),
                counter: row.get::<_, i64>(2).try_into().unwrap_or(u32::MAX),
                transports: Vec::new(),
                aaguid: [0; 16],
            })
            .collect::<Vec<_>>();
        let (challenge, state) = webauthn(rp_id, origin)
            .start_authentication_with_creds_for_user(actor.as_bytes(), &credentials);
        let ceremony_id = format!("pca_{}", SecretGenerator::token("ceremony")?);
        let state = serde_json::to_value(state).map_err(|_| PlatformError::Storage)?;
        self.client.execute(
            "INSERT INTO passkey_ceremonies(id,actor_id,kind,state,expires_at) VALUES($1,$2,'authentication',$3,clock_timestamp()+$4::bigint*interval '1 second')",
            &[&ceremony_id, &actor, &state, &CEREMONY_TTL_SECONDS],
        ).map_err(Self::database_error)?;
        Ok(json!({"ceremony_id": ceremony_id, "challenge": challenge}))
    }

    pub fn finish_passkey_authentication(
        &mut self,
        ceremony_id: &str,
        response: Value,
        rp_id: &str,
        origin: &str,
    ) -> Result<IssuedSession, PlatformError> {
        if !ceremony_id.starts_with("pca_") {
            return Err(PlatformError::Unauthorized);
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row = tx.query_opt(
            "SELECT actor_id,state FROM passkey_ceremonies WHERE id=$1 AND kind='authentication' AND consumed_at IS NULL AND expires_at>clock_timestamp() FOR UPDATE",
            &[&ceremony_id],
        ).map_err(Self::database_error)?.ok_or_else(ceremony_error)?;
        let actor: String = row.get(0);
        let state: AuthenticationState =
            serde_json::from_value(row.get(1)).map_err(|_| ceremony_error())?;
        let response: AuthenticationResponse =
            serde_json::from_value(response).map_err(|_| ceremony_error())?;
        let credential_id =
            CredentialId::from_b64url(&response.id).map_err(|_| ceremony_error())?;
        let credential_row = tx.query_opt(
            "SELECT public_key_cose,sign_count FROM passkey_credentials WHERE actor_id=$1 AND credential_id=$2 FOR UPDATE",
            &[&actor, &credential_id.0],
        ).map_err(Self::database_error)?.ok_or_else(ceremony_error)?;
        let credential = PasskeyCredential {
            id: credential_id,
            public_key_cose: CosePublicKey(credential_row.get(0)),
            counter: credential_row
                .get::<_, i64>(1)
                .try_into()
                .unwrap_or(u32::MAX),
            transports: Vec::new(),
            aaguid: [0; 16],
        };
        let success = webauthn(rp_id, origin)
            .finish_authentication(&state, &response, &credential)
            .map_err(|_| ceremony_error())?;
        let id = SecretGenerator::id("ses")?;
        let token = format!("nxs_{id}.{}", SecretGenerator::token("secret")?);
        let digest = SecretGenerator::digest(&token);
        tx.execute("UPDATE passkey_credentials SET sign_count=$3,last_used_at=clock_timestamp() WHERE actor_id=$1 AND credential_id=$2", &[&actor, &success.credential_id.0, &i64::from(success.new_counter)]).map_err(Self::database_error)?;
        tx.execute(
            "UPDATE passkey_ceremonies SET consumed_at=clock_timestamp() WHERE id=$1",
            &[&ceremony_id],
        )
        .map_err(Self::database_error)?;
        let affected = tx.execute("INSERT INTO sessions(id,actor_id,digest,expires_at,access_stage) VALUES($1,$2,$3,clock_timestamp()+interval '8 hours','ready')", &[&id, &actor, &digest]).map_err(Self::database_error)?;
        if affected != 1 {
            return Err(PlatformError::Unauthorized);
        }
        AuditWriter::append(
            &mut tx,
            Some(&actor),
            Some(&id),
            "session.created",
            Some(&id),
            json!({"method":"passkey"}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(IssuedSession {
            access_token: token,
            token_type: "Bearer".into(),
            expires_in: 28800,
            actor_id: actor,
            next_step: None,
        })
    }
}
