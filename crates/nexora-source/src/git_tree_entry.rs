use crate::GitEntryKind;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTreeEntry {
    pub path: String,
    pub object_id: String,
    pub kind: GitEntryKind,
    pub size: Option<u64>,
}
