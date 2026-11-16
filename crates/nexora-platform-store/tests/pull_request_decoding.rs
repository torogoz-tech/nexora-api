use nexora_platform::GitProvider;
use nexora_platform_store::PullRequestDecoder;
use serde_json::json;
#[test]
fn github_and_gitlab_preserve_local_numbers_and_merged_state() {
    let gh=PullRequestDecoder::summary(GitProvider::Github,&json!({"number":12,"title":"Fix","state":"closed","merged_at":"2026-11-05","body":null,"user":{"login":"owner"},"head":{"ref":"fix","sha":"abc"},"base":{"ref":"main"},"secret":"never expose"})).unwrap();
    let gl=PullRequestDecoder::summary(GitProvider::Gitlab,&json!({"id":999,"iid":12,"title":"Fix","state":"merged","source_branch":"fix","target_branch":"main","author":{"username":"owner"}})).unwrap();
    assert_eq!(gh["number"], gl["number"]);
    assert_eq!(gh["state"], "merged");
    assert_eq!(gl["state"], "merged");
    assert_eq!(gh["description"], "");
    assert!(gh.get("secret").is_none());
    assert!(
        PullRequestDecoder::summary(GitProvider::Github, &json!({"number":0,"title":"Invalid"}))
            .is_err()
    );
}
#[test]
fn incomplete_and_oversized_diffs_are_marked() {
    let missing =
        PullRequestDecoder::file(GitProvider::Github, &json!({"filename":"binary.png"})).unwrap();
    assert_eq!(missing["patch_truncated"], true);
    assert!(missing["patch"].is_null());
    let text = "λ".repeat(40000);
    let large = PullRequestDecoder::file(
        GitProvider::Gitlab,
        &json!({"new_path":"src/file.rs","old_path":"old.rs","renamed_file":true,"diff":text}),
    )
    .unwrap();
    assert_eq!(large["status"], "renamed");
    assert_eq!(large["patch_truncated"], true);
    assert_eq!(large["patch"].as_str().unwrap().chars().count(), 32768);
}
#[test]
fn inline_and_conversation_comments_do_not_collide() {
    let v = json!({"id":42,"body":"<script>untrusted</script>","path":"main.rs","line":7});
    let a = PullRequestDecoder::comment(GitProvider::Github, &v, false).unwrap();
    let b = PullRequestDecoder::comment(GitProvider::Github, &v, true).unwrap();
    assert_ne!(a["id"], b["id"]);
    assert!(a["path"].is_null());
    assert_eq!(b["line"], 7);
    assert_eq!(b["body"], v["body"]);
}
