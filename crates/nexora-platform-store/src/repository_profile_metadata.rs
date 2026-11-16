use nexora_platform::{GitProvider, PlatformError, RepositoryRegistration};
use serde_json::{Value, json};

pub(crate) struct RepositoryProfileMetadata;
impl RepositoryProfileMetadata {
    pub fn decode(
        provider: GitProvider,
        remote_id: &str,
        value: &Value,
    ) -> Result<Value, PlatformError> {
        let actual = value["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or(PlatformError::Unavailable)?
            .to_string();
        if actual != remote_id {
            return Err(PlatformError::NotFound);
        }
        let (name_key, url_key) = match provider {
            GitProvider::Github => ("full_name", "html_url"),
            GitProvider::Gitlab => ("path_with_namespace", "web_url"),
        };
        let name = value[name_key].as_str().ok_or(PlatformError::Unavailable)?;
        RepositoryRegistration {
            provider,
            full_name: name.into(),
        }
        .locator()
        .map_err(|_| PlatformError::Unavailable)?;
        let text = |key: &str, limit: usize| {
            value[key]
                .as_str()
                .filter(|text| text.len() <= limit && !text.chars().any(char::is_control))
        };
        let web_url = text(url_key, 2048).filter(|text| {
            url::Url::parse(text).is_ok_and(|url| {
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            })
        });
        let description = value["description"].as_str().filter(|text| {
            text.len() <= 4096
                && text.chars().all(|character| {
                    !character.is_control() || matches!(character, '\n' | '\r' | '\t')
                })
        });
        Ok(
            json!({"id":null,"remote_id":remote_id,"name":name,"description":description,"provider":provider,"status":"available","default_branch":text("default_branch",512),"web_url":web_url}),
        )
    }
}
