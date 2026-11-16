use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use std::path::Path;
pub struct DatabaseRunner;
impl DatabaseRunner {
    pub fn connect(path: &Path) -> Result<DatabaseRuntime, Box<dyn std::error::Error>> {
        Ok(DatabaseRuntime::connect(&DatabaseConnections::load(path)?)?)
    }
}
