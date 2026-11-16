use super::TaskReleaseComposition;
use nexora_platform::GitProvider;
use serde_json::json;
#[test]
fn github_requires_selected_task_commit_as_the_verified_common_ancestor() {
    let source = "1".repeat(40);
    assert!(TaskReleaseComposition::contains(
        GitProvider::Github,
        &json!({"merge_base_commit":{"sha":source}}),
        &source
    ));
    assert!(!TaskReleaseComposition::contains(
        GitProvider::Github,
        &json!({"merge_base_commit":{"sha":"2".repeat(40)}}),
        &source
    ));
    assert!(!TaskReleaseComposition::contains(
        GitProvider::Github,
        &json!({"status":"ahead"}),
        &source
    ));
}
#[test]
fn gitlab_requires_selected_task_commit_as_the_verified_merge_base() {
    let source = "1".repeat(40);
    assert!(TaskReleaseComposition::contains(
        GitProvider::Gitlab,
        &json!({"id":source}),
        &source
    ));
    assert!(!TaskReleaseComposition::contains(
        GitProvider::Gitlab,
        &json!({"id":"2".repeat(40)}),
        &source
    ));
    assert!(!TaskReleaseComposition::contains(
        GitProvider::Gitlab,
        &json!({"commit":{"id":source}}),
        &source
    ));
}
