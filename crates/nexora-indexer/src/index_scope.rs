#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexScope {
    pub tenant: nexora_core::TenantId,
    pub repository: nexora_core::RepositoryId,
    pub root: std::path::PathBuf,
    pub configuration: String,
}
