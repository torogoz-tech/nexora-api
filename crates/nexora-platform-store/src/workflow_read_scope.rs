pub(crate) enum WorkflowReadScope {
    All,
    Assigned(Vec<String>),
    Unassigned,
}
impl WorkflowReadScope {
    pub fn permits(&self, id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Assigned(assigned) => assigned.iter().any(|workflow| workflow == id),
            Self::Unassigned => false,
        }
    }
}
