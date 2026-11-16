use nexora_core::NexoraError;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConnections {
    pub schema_version: u16,
    pub host: String,
    pub postgres_port: u16,
    pub redis_port: u16,
    pub neo4j_http_port: u16,
    pub postgres_database: String,
    pub postgres_user: String,
    pub secrets_directory: PathBuf,
}
impl DatabaseConnections {
    pub fn load(path: &Path) -> Result<Self, NexoraError> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(16_385)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16_384 {
            return Err(NexoraError::Configuration(
                "connection settings >16KiB".into(),
            ));
        }
        let mut settings: Self = serde_json::from_slice(&bytes)
            .map_err(|_| NexoraError::Configuration("invalid connection settings".into()))?;
        settings.validate()?;
        if settings.secrets_directory.is_relative() {
            settings.secrets_directory = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(&settings.secrets_directory);
        }
        Ok(settings)
    }
    pub fn validate(&self) -> Result<(), NexoraError> {
        if self.schema_version != 1
            || !["127.0.0.1", "localhost"].contains(&self.host.as_str())
            || self.postgres_port == 0
            || self.redis_port == 0
            || self.neo4j_http_port == 0
            || self.postgres_user.is_empty()
            || self.postgres_database.is_empty()
        {
            return Err(NexoraError::Configuration(
                "invalid local database settings".into(),
            ));
        }
        Ok(())
    }
}
