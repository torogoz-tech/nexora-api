#[path = "support/git_fixture.rs"]
mod git_fixture;
#[path = "support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use nexora_source::{GitReleaseComposer, GitTransferLimits};
#[test]
fn composition_applies_only_selected_patches_onto_main_and_excludes_unselected_changes() {
    let git = GitFixture::new("release-selected");
    let original = git.blob(b"main\n");
    let base_tree = git.tree(&[("100644", &original, "main.txt")]);
    let main = git.commit(&base_tree);
    let selected_blob = git.blob(b"selected\n");
    let selected_tree = git.tree(&[
        ("100644", &original, "main.txt"),
        ("100644", &selected_blob, "selected.txt"),
    ]);
    let selected = String::from_utf8(git.run(
        &["commit-tree", &selected_tree, "-p", &main, "-m", "Selected"],
        b"",
    ))
    .unwrap()
    .trim()
    .to_owned();
    let unrelated = git.blob(b"unselected\n");
    let stage_tree = git.tree(&[
        ("100644", &original, "main.txt"),
        ("100644", &selected_blob, "selected.txt"),
        ("100644", &unrelated, "unselected.txt"),
    ]);
    let _stage = String::from_utf8(git.run(
        &[
            "commit-tree",
            &stage_tree,
            "-p",
            &selected,
            "-m",
            "Unselected",
        ],
        b"",
    ))
    .unwrap();
    let composer =
        GitReleaseComposer::new("/usr/bin/git".into(), GitTransferLimits::default()).unwrap();
    let release = composer
        .compose(
            &git.bare,
            &main,
            std::slice::from_ref(&selected),
            "Release selected only\n",
        )
        .unwrap();
    let names = String::from_utf8(git.run(&["ls-tree", "--name-only", &release], b"")).unwrap();
    assert!(names.contains("selected.txt"));
    assert!(!names.contains("unselected.txt"));
    assert!(names.contains("main.txt"));
    let parents =
        String::from_utf8(git.run(&["show", "-s", "--format=%P", &release], b"")).unwrap();
    assert_eq!(parents.trim(), main);
    assert_eq!(
        release,
        composer
            .compose(&git.bare, &main, &[selected], "Release selected only\n")
            .unwrap()
    );
}
#[test]
fn missing_dependency_or_conflict_fails_without_releasing_unselected_changes() {
    let git = GitFixture::new("release-conflict");
    let original = git.blob(b"base\n");
    let base_tree = git.tree(&[("100644", &original, "a.txt")]);
    let main = git.commit(&base_tree);
    let one = git.blob(b"first\n");
    let tree = git.tree(&[("100644", &one, "a.txt")]);
    let first =
        String::from_utf8(git.run(&["commit-tree", &tree, "-p", &main, "-m", "First"], b""))
            .unwrap()
            .trim()
            .to_owned();
    let two = git.blob(b"second\n");
    let tree = git.tree(&[("100644", &two, "a.txt")]);
    let second =
        String::from_utf8(git.run(&["commit-tree", &tree, "-p", &main, "-m", "Second"], b""))
            .unwrap()
            .trim()
            .to_owned();
    let composer =
        GitReleaseComposer::new("/usr/bin/git".into(), GitTransferLimits::default()).unwrap();
    assert!(
        composer
            .compose(&git.bare, &main, &[first, second], "Conflicting release\n")
            .is_err()
    );
    assert_eq!(
        String::from_utf8(git.run(&["for-each-ref", "--format=%(refname)"], b""))
            .unwrap_or_default(),
        ""
    );
}
#[test]
fn an_unselected_parent_commit_is_not_imported_when_selecting_its_child_patch() {
    let git = GitFixture::new("release-unselected-parent");
    let original = git.blob(b"main\n");
    let tree = git.tree(&[("100644", &original, "main.txt")]);
    let main = git.commit(&tree);
    let other = git.blob(b"not approved\n");
    let tree = git.tree(&[
        ("100644", &original, "main.txt"),
        ("100644", &other, "unselected.txt"),
    ]);
    let parent = String::from_utf8(git.run(
        &["commit-tree", &tree, "-p", &main, "-m", "Other task"],
        b"",
    ))
    .unwrap()
    .trim()
    .to_owned();
    let own = git.blob(b"approved\n");
    let tree = git.tree(&[
        ("100644", &original, "main.txt"),
        ("100644", &other, "unselected.txt"),
        ("100644", &own, "selected.txt"),
    ]);
    let selected = String::from_utf8(git.run(
        &["commit-tree", &tree, "-p", &parent, "-m", "Selected task"],
        b"",
    ))
    .unwrap()
    .trim()
    .to_owned();
    std::fs::write(git.bare.join("shallow"), format!("{selected}\n")).unwrap();
    let composer =
        GitReleaseComposer::new("/usr/bin/git".into(), GitTransferLimits::default()).unwrap();
    let release = composer
        .compose(&git.bare, &main, &[selected], "Explicit selected patch\n")
        .unwrap();
    let names = String::from_utf8(git.run(&["ls-tree", "--name-only", &release], b"")).unwrap();
    assert!(names.contains("selected.txt"));
    assert!(!names.contains("unselected.txt"));
}
#[path = "support/git_http_fixture.rs"]
mod git_http_fixture;
#[test]
fn composed_branch_push_is_authenticated_create_only_and_never_overwrites_existing_release() {
    use nexora_source::{GitFetchCredentials, GitFetchPolicy, GitFetcher, GitObjectFormat};
    let git = GitFixture::new("release-http-push");
    let base_blob = git.blob(b"production\n");
    let base_tree = git.tree(&[("100644", &base_blob, "main.txt")]);
    let main = git.commit(&base_tree);
    let chosen = git.blob(b"approved\n");
    let chosen_tree = git.tree(&[
        ("100644", &base_blob, "main.txt"),
        ("100644", &chosen, "selected.txt"),
    ]);
    let commit = String::from_utf8(git.run(
        &["commit-tree", &chosen_tree, "-p", &main, "-m", "Selected"],
        b"",
    ))
    .unwrap()
    .trim()
    .to_owned();
    git.run(&["update-ref", "refs/heads/main", &main], b"");
    git.run(&["update-ref", "refs/heads/selected", &commit], b"");
    git.run(&["symbolic-ref", "HEAD", "refs/heads/main"], b"");
    let server = git_http_fixture::GitHttpFixture::start(&git);
    let credentials = GitFetchCredentials::new("oauth2", server.token().to_owned()).unwrap();
    let policy = GitFetchPolicy::new(server.origin()).unwrap();
    let limits = GitTransferLimits::default();
    let fetcher = GitFetcher::new("/usr/bin/git".into(), limits.clone()).unwrap();
    let source = fetcher
        .fetch(
            &policy,
            "team/source",
            &main,
            GitObjectFormat::Sha1,
            &credentials,
            &git.directory.path().join("release-fetch"),
        )
        .unwrap();
    fetcher
        .fetch_into(
            &policy,
            "team/source",
            &commit,
            GitObjectFormat::Sha1,
            &credentials,
            source.bare(),
        )
        .unwrap();
    let composer = GitReleaseComposer::new("/usr/bin/git".into(), limits).unwrap();
    let head = composer
        .compose(
            source.bare(),
            &main,
            &[commit],
            "Selected production release\n",
        )
        .unwrap();
    composer
        .push_new(
            source.bare(),
            &policy,
            "team/source",
            &credentials,
            &head,
            "release/selected",
        )
        .unwrap();
    assert!(
        composer
            .push_new(
                source.bare(),
                &policy,
                "team/source",
                &credentials,
                &main,
                "release/selected"
            )
            .is_err()
    );
    let remote = git.directory.path().join("server/team/source.git");
    let output = std::process::Command::new("/usr/bin/git")
        .arg("-C")
        .arg(remote)
        .args(["rev-parse", "refs/heads/release/selected"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), head);
    assert!(
        !std::fs::read_to_string(source.bare().join("config"))
            .unwrap()
            .contains(server.token())
    );
    assert_eq!(server.trap_requests(), 0);
}
