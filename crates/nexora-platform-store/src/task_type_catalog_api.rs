use crate::{
    PlatformStore, project_task_type_selection_store::ProjectTaskTypeSelectionStore,
    task_type_catalog_store::TaskTypeCatalogStore,
};
use nexora_platform::*;
impl PlatformStore {
    pub fn catalog_task_types(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<CatalogTaskTypePage, PlatformError> {
        TaskTypeCatalogStore::page(self, ctx, after, limit)
    }
    pub fn create_catalog_task_type(
        &mut self,
        ctx: &AuthContext,
        input: &CatalogTaskTypeCreate,
    ) -> Result<CatalogTaskType, PlatformError> {
        TaskTypeCatalogStore::create(self, ctx, input)
    }
    pub fn change_catalog_task_type(
        &mut self,
        ctx: &AuthContext,
        code: &str,
        input: &CatalogTaskTypeChange,
    ) -> Result<CatalogTaskType, PlatformError> {
        TaskTypeCatalogStore::change(self, ctx, code, input)
    }
    pub fn project_task_type_selection(
        &mut self,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<ProjectTaskTypeSelection, PlatformError> {
        ProjectTaskTypeSelectionStore::get(self, ctx, project)
    }
    pub fn change_project_task_type_selection(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectTaskTypeSelectionChange,
    ) -> Result<ProjectTaskTypeSelection, PlatformError> {
        ProjectTaskTypeSelectionStore::change(self, ctx, project, input)
    }
}
