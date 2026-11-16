#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitEntryKind {
    File,
    ExecutableFile,
    SymbolicLink,
    Submodule,
}
