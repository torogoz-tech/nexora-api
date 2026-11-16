#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/git_fixture.rs"]
mod git_fixture;
#[path = "support/git_http_fixture.rs"]
mod git_http_fixture;
#[path = "support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use git_http_fixture::GitHttpFixture;
use nexora_source::{
    GitFetchCredentials, GitFetchPolicy, GitFetcher, GitMaterializer, GitObjectFormat,
    GitObjectReader, GitTransferLimits, SourceLimits,
};
use std::{io::Read, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

#[test]
fn authenticated_fetch_pins_commit_refuses_redirects_and_keeps_credentials_out_of_args_and_config()
{
    let fixture = GitFixture::new("http-fetch");
    let body = b"pub fn captured() {}\n";
    let code = fixture.blob(body);
    let mut binary = vec![0; 16_384];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut binary)
        .unwrap();
    let binary_oid = fixture.blob(&binary);
    let tree = fixture.tree(&[
        ("100644", &code, "main.rs"),
        ("100644", &binary_oid, "data.bin"),
    ]);
    let commit = fixture.commit(&tree);
    fixture.run(&["update-ref", "refs/heads/main", &commit], b"");
    fixture.run(&["symbolic-ref", "HEAD", "refs/heads/main"], b"");
    let server = GitHttpFixture::start(&fixture);
    let credentials = GitFetchCredentials::new("oauth2", server.token().to_owned()).unwrap();
    let policy = GitFetchPolicy::new(server.origin()).unwrap();
    for origin in [
        "http://example.invalid",
        "http://localhost:1234",
        "https://example.invalid/path",
        "https://u:p@example.invalid",
        "https://example.invalid?query=1",
    ] {
        assert!(GitFetchPolicy::new(origin).is_err());
    }
    let tool = fixture.directory.path().join("fetch-git");
    std::fs::write(
        &tool,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$0.args\"\nexec /usr/bin/git \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    let fetcher = GitFetcher::new(tool.clone(), GitTransferLimits::default()).unwrap();
    let destination = fixture.directory.path().join("download");
    let fetched = fetcher
        .fetch(
            &policy,
            "team/source",
            &commit,
            GitObjectFormat::Sha1,
            &credentials,
            &destination,
        )
        .unwrap();
    assert_eq!(fetched.commit(), commit);
    let config = std::fs::read_to_string(fetched.bare().join("config")).unwrap();
    let arguments = std::fs::read_to_string(tool.with_extension("args")).unwrap();
    for value in [&config, &arguments] {
        assert!(!value.contains(server.token()));
        assert!(!value.to_ascii_lowercase().contains("extraheader"));
        assert!(!value.to_ascii_lowercase().contains("authorization:"));
    }
    assert!(!fetched.bare().join("FETCH_HEAD").exists());
    let reader =
        GitObjectReader::new(PathBuf::from("/usr/bin/git"), Duration::from_secs(10)).unwrap();
    let capture = fixture.directory.path().join("materialized");
    GitMaterializer::materialize(
        &reader,
        fetched.bare(),
        &commit,
        GitObjectFormat::Sha1,
        &capture,
        &SourceLimits::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(capture.join("main.rs")).unwrap(), body);
    assert_eq!(std::fs::read(capture.join("data.bin")).unwrap(), binary);
    std::fs::write(destination.join("keep"), b"existing").unwrap();
    assert!(
        fetcher
            .fetch(
                &policy,
                "team/source",
                &commit,
                GitObjectFormat::Sha1,
                &credentials,
                &destination
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(destination.join("keep")).unwrap(),
        b"existing"
    );
    let wrong =
        GitFetchCredentials::new("oauth2", format!("wrong-{}", std::process::id())).unwrap();
    let target = fixture.directory.path().join("wrong-credentials");
    assert!(
        fetcher
            .fetch(
                &policy,
                "team/source",
                &commit,
                GitObjectFormat::Sha1,
                &wrong,
                &target
            )
            .is_err()
    );
    assert!(!target.exists());
    let redirect = fixture.directory.path().join("redirect");
    assert!(
        fetcher
            .fetch(
                &policy,
                "team/redirect",
                &commit,
                GitObjectFormat::Sha1,
                &credentials,
                &redirect
            )
            .is_err()
    );
    assert!(!redirect.exists());
    assert_eq!(server.trap_requests(), 0);
    let missing = fixture.directory.path().join("missing");
    assert!(
        fetcher
            .fetch(
                &policy,
                "team/source",
                &"f".repeat(40),
                GitObjectFormat::Sha1,
                &credentials,
                &missing
            )
            .is_err()
    );
    assert!(!missing.exists());
    let small = GitFetcher::new(
        PathBuf::from("/usr/bin/git"),
        GitTransferLimits {
            max_pack_bytes: 1024,
            max_repository_bytes: 1024 * 1024,
            ..GitTransferLimits::default()
        },
    )
    .unwrap();
    let over = fixture.directory.path().join("over-budget");
    assert!(
        small
            .fetch(
                &policy,
                "team/source",
                &commit,
                GitObjectFormat::Sha1,
                &credentials,
                &over
            )
            .is_err()
    );
    assert!(!over.exists());
}
