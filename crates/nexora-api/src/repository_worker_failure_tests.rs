use super::RepositoryWorker;
use nexora_core::NexoraError;
use nexora_platform::RepositoryJobFailure;

#[test]
fn staging_storage_errors_are_retryable_but_analysis_failures_are_terminal() {
    assert!(matches!(
        RepositoryWorker::analysis_failure(NexoraError::Storage("unavailable".into())),
        RepositoryJobFailure::StorageUnavailable
    ));
    for error in [
        NexoraError::Io(std::io::Error::other("source I/O")),
        NexoraError::Configuration("invalid model".into()),
        NexoraError::Lexical("invalid source".into()),
        NexoraError::Limit("source limit".into()),
        NexoraError::InvalidPath("invalid path".into()),
    ] {
        assert!(matches!(
            RepositoryWorker::analysis_failure(error),
            RepositoryJobFailure::AnalysisFailed
        ));
    }
}
