use crate::{PlatformStore, RepositoryJobLease, analysis_profile_store::AnalysisProfileStore};
use nexora_language::AnalysisProfilePackage;
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn import_analysis_profile(
        &mut self,
        ctx: &AuthContext,
        p: &AnalysisProfilePackage,
    ) -> Result<Value, PlatformError> {
        AnalysisProfileStore::import(self, ctx, p)
    }
    pub fn analysis_profiles(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<Value, PlatformError> {
        AnalysisProfileStore::list(self, ctx, after, limit)
    }
    pub fn analysis_profile(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        export: bool,
    ) -> Result<Value, PlatformError> {
        AnalysisProfileStore::get(self, ctx, id, export)
    }
    pub fn repository_analysis_profile(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repo: &str,
    ) -> Result<Value, PlatformError> {
        AnalysisProfileStore::assignment(self, ctx, project, repo, None)
    }
    pub fn change_repository_analysis_profile(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repo: &str,
        input: &RepositoryAnalysisProfileChange,
    ) -> Result<Value, PlatformError> {
        AnalysisProfileStore::assignment(self, ctx, project, repo, Some(input))
    }
    pub fn repository_job_analysis_profiles(
        &mut self,
        lease: &RepositoryJobLease,
    ) -> Result<Option<Value>, PlatformError> {
        crate::analysis_profile_selection_store::AnalysisProfileSelectionStore::load(self, lease)
    }
    pub fn repository_job_analysis_profile(
        &mut self,
        lease: &RepositoryJobLease,
    ) -> Result<Option<AnalysisProfilePackage>, PlatformError> {
        AnalysisProfileStore::job(self, lease)
    }
}
