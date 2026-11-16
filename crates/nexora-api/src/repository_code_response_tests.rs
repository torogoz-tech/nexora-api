use crate::repository_code_handlers::RepositoryCodeHandlers;
use nexora_platform::PlatformError;
use serde_json::json;

#[test]
fn response_budget_counts_encoded_json_bytes() {
    let overhead = serde_json::to_vec(&json!({"value":""})).unwrap().len();
    let exact = json!({"value":"x".repeat(1_048_576-overhead)});
    assert_eq!(
        RepositoryCodeHandlers::encode(&exact).unwrap().len(),
        1_048_576
    );
    let oversized = json!({"value":"x".repeat(1_048_577-overhead)});
    assert!(matches!(
        RepositoryCodeHandlers::encode(&oversized),
        Err(PlatformError::Validation(_))
    ));
    let escaped = json!({"value":"\0".repeat(200_000)});
    assert!(matches!(
        RepositoryCodeHandlers::encode(&escaped),
        Err(PlatformError::Validation(_))
    ));
}
