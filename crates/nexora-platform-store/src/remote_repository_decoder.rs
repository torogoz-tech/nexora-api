use nexora_platform::*;
use serde_json::Value;

pub(crate) struct RemoteRepositoryDecoder;
impl RemoteRepositoryDecoder {
    pub fn validate(page: u32, limit: u16) -> Result<(), PlatformError> {
        if page == 0 || page > 100_000 || !(1..=100).contains(&limit) {
            return Err(PlatformError::Validation("invalid remote repository page"));
        }
        Ok(())
    }
    pub fn decode(
        provider: GitProvider,
        value: &Value,
        page: u32,
        limit: u16,
    ) -> Result<RemoteRepositoryPage, PlatformError> {
        Self::validate(page, limit)?;
        let rows = match provider {
            GitProvider::Github => value["repositories"].as_array(),
            GitProvider::Gitlab => value.as_array(),
        }
        .filter(|rows| rows.len() <= usize::from(limit))
        .ok_or(PlatformError::Unavailable)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let remote_id = row["id"]
                .as_u64()
                .filter(|id| *id > 0)
                .ok_or(PlatformError::Unavailable)?
                .to_string();
            let key = match provider {
                GitProvider::Github => "full_name",
                GitProvider::Gitlab => "path_with_namespace",
            };
            let name = row[key].as_str().ok_or(PlatformError::Unavailable)?;
            RepositoryRegistration {
                provider,
                full_name: name.into(),
            }
            .locator()
            .map_err(|_| PlatformError::Unavailable)?;
            items.push(RemoteRepositoryMetadata {
                provider,
                remote_id,
                full_name: name.into(),
            });
        }
        let next_page = if rows.len() == usize::from(limit) {
            if page == 100_000 {
                return Err(PlatformError::Unavailable);
            }
            Some(page + 1)
        } else {
            None
        };
        Ok(RemoteRepositoryPage { items, next_page })
    }
}
