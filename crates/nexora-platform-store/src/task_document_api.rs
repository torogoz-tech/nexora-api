use crate::{PlatformStore, task_document_store::TaskDocumentStore};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
impl PlatformStore {
    pub fn upload_task_document(
        &mut self,
        ctx: &AuthContext,
        task: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, PlatformError> {
        TaskDocumentStore::upload(self, ctx, task, name, bytes)
    }
    pub fn task_document_page(
        &mut self,
        ctx: &AuthContext,
        task: Option<&str>,
        project: Option<&str>,
        search: &str,
        after: Option<&str>,
    ) -> Result<Value, PlatformError> {
        TaskDocumentStore::page(self, ctx, task, project, search, after)
    }
    pub fn task_document_content(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(String, Vec<u8>), PlatformError> {
        TaskDocumentStore::content(self, ctx, id)
    }
}
