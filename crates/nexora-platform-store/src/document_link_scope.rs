use nexora_platform::{ProjectDocument, WorkTask};
pub(crate) struct DocumentLinkScope {
    pub source: ProjectDocument,
    pub target_document: Option<ProjectDocument>,
    pub task: Option<WorkTask>,
}
