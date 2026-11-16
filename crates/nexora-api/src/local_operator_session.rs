use crate::{LocalOperatorArguments, local_operator_secret::LocalOperatorSecret};
use nexora_platform::{ActorKind, AuthContext, AuthMethod, PlatformError};
use nexora_platform_store::PlatformStore;
use zeroize::Zeroizing;

pub(crate) struct LocalOperatorSession {
    pub context: AuthContext,
    token: Zeroizing<String>,
    owned: bool,
}
impl LocalOperatorSession {
    pub fn open(
        store: &mut PlatformStore,
        args: &LocalOperatorArguments,
    ) -> Result<Self, PlatformError> {
        if args.session_file.is_some() && (args.username.is_some() || args.password_file.is_some())
        {
            return Err(PlatformError::Validation(
                "choose one operator authentication method",
            ));
        }
        let (token, owned) = if let Some(path) = &args.session_file {
            let token = LocalOperatorSecret::read(path)?;
            if !token.bytes().all(|b| b.is_ascii_graphic()) {
                return Err(PlatformError::Validation(
                    "operator session must be one bounded token",
                ));
            }
            (token, false)
        } else {
            let username = args
                .username
                .as_deref()
                .ok_or(PlatformError::Validation("operator login is required"))?;
            let path = args
                .password_file
                .as_deref()
                .ok_or(PlatformError::Validation(
                    "operator password file is required",
                ))?;
            let password = LocalOperatorSecret::read(path)?;
            (
                Zeroizing::new(store.login(username, &password)?.access_token),
                true,
            )
        };
        let context = store.authenticate(&token);
        let context = match context {
            Ok(ctx) if ctx.kind() == ActorKind::Human && ctx.method() == AuthMethod::Session => ctx,
            result => {
                if owned {
                    store.logout(&token)?;
                }
                return Err(result.err().unwrap_or(PlatformError::Forbidden));
            }
        };
        Ok(Self {
            context,
            token,
            owned,
        })
    }
    pub fn close(self, store: &mut PlatformStore) -> Result<(), PlatformError> {
        if self.owned {
            store.logout(&self.token)?;
        }
        Ok(())
    }
}
