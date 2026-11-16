#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/git_fixture.rs"]
mod git_fixture;
#[path = "support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use nexora_source::{GitMaterializer, GitObjectFormat, GitObjectReader, SourceError, SourceLimits};
use std::{
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    time::Duration,
};

#[test]
fn materialization_preserves_exact_files_privacy_and_existing_destinations_without_following_links()
{
    let fixture = GitFixture::new("materialization");
    let code = b"public class Captured {}\n";
    let revision = b"$Format:%H$\n";
    let script = b"#!/bin/sh\nexit 77\n";
    let attributes = b"src/* export-ignore\nrevision.txt export-subst\n";
    let code_oid = fixture.blob(code);
    let revision_oid = fixture.blob(revision);
    let script_oid = fixture.blob(script);
    let empty_oid = fixture.blob(b"");
    let attributes_oid = fixture.blob(attributes);
    let child = fixture.tree(&[("100644", &code_oid, "Niño With Spaces.cs")]);
    let tree = fixture.tree(&[
        ("040000", &child, "src"),
        ("100644", &revision_oid, "revision.txt"),
        ("100755", &script_oid, "build.sh"),
        ("100644", &empty_oid, "empty.txt"),
        ("100644", &attributes_oid, ".gitattributes"),
    ]);
    let commit = fixture.commit(&tree);
    let reader =
        GitObjectReader::new(PathBuf::from("/usr/bin/git"), Duration::from_secs(10)).unwrap();
    let destination = fixture.directory.path().join("captured");
    let captured = GitMaterializer::materialize(
        &reader,
        &fixture.bare,
        &commit,
        GitObjectFormat::Sha1,
        &destination,
        &SourceLimits::default(),
    )
    .unwrap();
    assert_eq!(captured.commit(), commit);
    assert_eq!(captured.files().len(), 5);
    assert_eq!(captured.root(), destination);
    assert_eq!(
        std::fs::read(destination.join("src/Niño With Spaces.cs")).unwrap(),
        code
    );
    assert_eq!(
        std::fs::read(destination.join("revision.txt")).unwrap(),
        revision
    );
    assert_eq!(std::fs::read(destination.join("build.sh")).unwrap(), script);
    assert_eq!(std::fs::read(destination.join("empty.txt")).unwrap(), b"");
    assert_eq!(
        std::fs::metadata(&destination)
            .unwrap()
            .permissions()
            .mode()
            & 0o077,
        0
    );
    assert_eq!(
        std::fs::metadata(destination.join("build.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
    assert_eq!(
        captured.total_bytes(),
        (code.len() + revision.len() + script.len() + attributes.len()) as u64
    );
    std::fs::write(destination.join("keep"), b"existing").unwrap();
    assert!(
        GitMaterializer::materialize(
            &reader,
            &fixture.bare,
            &commit,
            GitObjectFormat::Sha1,
            &destination,
            &SourceLimits::default()
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(destination.join("keep")).unwrap(),
        b"existing"
    );
    let alias = fixture.directory.path().join("alias");
    symlink(&destination, &alias).unwrap();
    assert!(
        GitMaterializer::materialize(
            &reader,
            &fixture.bare,
            &commit,
            GitObjectFormat::Sha1,
            &alias,
            &SourceLimits::default()
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(destination.join("keep")).unwrap(),
        b"existing"
    );
    let link_oid = fixture.blob(b"../outside");
    for (mode, oid, name) in [
        ("120000", link_oid.as_str(), "link"),
        (
            "160000",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "submodule",
        ),
    ] {
        let unsupported = fixture.commit(&fixture.tree(&[(mode, oid, name)]));
        let target = fixture.directory.path().join(name);
        assert!(
            GitMaterializer::materialize(
                &reader,
                &fixture.bare,
                &unsupported,
                GitObjectFormat::Sha1,
                &target,
                &SourceLimits::default()
            )
            .is_err()
        );
        assert!(
            !target.exists(),
            "Unsupported entry left a partial source directory"
        );
    }
    // The metadata is valid, but the full destination path exceeds the host filesystem limit.
    // A short file sorts first, so the writer must clean its owned directory after a later error.
    let component = "z".repeat(200);
    let mut deep = fixture.tree(&[("100644", &code_oid, "last.cs")]);
    for _ in 0..20 {
        deep = fixture.tree(&[("040000", &deep, &component)]);
    }
    let failing = fixture.commit(&fixture.tree(&[
        ("100644", &code_oid, "a-short.cs"),
        ("040000", &deep, "zTop"),
    ]));
    assert_eq!(
        reader
            .tree(
                &fixture.bare,
                &failing,
                GitObjectFormat::Sha1,
                &SourceLimits::default()
            )
            .unwrap()
            .len(),
        2
    );
    let partial = fixture
        .directory
        .path()
        .join(format!("partial-{}", "x".repeat(200)));
    assert!(matches!(
        GitMaterializer::materialize(
            &reader,
            &fixture.bare,
            &failing,
            GitObjectFormat::Sha1,
            &partial,
            &SourceLimits::default()
        ),
        Err(SourceError::Unavailable)
    ));
    assert!(
        !partial.exists(),
        "A filesystem write failure left a partial source capture"
    );
}
