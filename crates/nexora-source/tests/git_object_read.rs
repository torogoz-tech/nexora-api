#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/test_directory.rs"]
mod test_directory;
use nexora_source::{GitEntryKind, GitObjectFormat, GitObjectReader, GitTreeEntry, SourceLimits};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
use test_directory::TestDirectory;

#[test]
fn raw_reader_preserves_export_ignored_and_substituted_bytes_without_running_filters() {
    let folder = TestDirectory::new("object-reader");
    let bare = folder.path().join("fixture.git");
    assert!(
        Command::new("/usr/bin/git")
            .env_clear()
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .args(["init", "--bare", "--template=", "--object-format=sha1"])
            .arg(&bare)
            .output()
            .unwrap()
            .status
            .success()
    );
    let git = |args: &[&str], data: &[u8]| {
        let mut child = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(&bare)
            .args(args)
            .env_clear()
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(data).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "Controlled fixture construction failed"
        );
        result.stdout
    };
    let attributes = b"hidden.rs export-ignore\nrevision.txt export-subst\n*.rs filter=forbidden\n";
    let hidden = b"pub fn retained() {}\n";
    let revision = b"$Format:%H$\n";
    let mut objects = Vec::new();
    for (name, bytes) in [
        (".gitattributes", attributes.as_slice()),
        ("hidden.rs", hidden.as_slice()),
        ("revision.txt", revision.as_slice()),
    ] {
        let oid = String::from_utf8(git(
            &["hash-object", "-w", "--stdin", "--no-filters"],
            bytes,
        ))
        .unwrap()
        .trim()
        .to_owned();
        objects.push((name, oid, bytes));
    }
    let tree = objects
        .iter()
        .map(|(name, oid, _)| format!("100644 blob {oid}\t{name}\0"))
        .collect::<String>();
    let tree = String::from_utf8(git(&["mktree", "-z"], tree.as_bytes()))
        .unwrap()
        .trim()
        .to_owned();
    let commit = String::from_utf8(git(&["commit-tree", &tree, "-m", "Fixture"], b""))
        .unwrap()
        .trim()
        .to_owned();
    let marker = folder.path().join("filter-ran");
    let filter = format!("touch {}", marker.display());
    git(&["config", "filter.forbidden.smudge", &filter], b"");
    git(&["config", "filter.forbidden.required", "true"], b"");
    let reader =
        GitObjectReader::new(PathBuf::from("/usr/bin/git"), Duration::from_secs(10)).unwrap();
    let entries = reader
        .tree(
            &bare,
            &commit,
            GitObjectFormat::Sha1,
            &SourceLimits::default(),
        )
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().any(|entry| entry.path == "hidden.rs"));
    use std::os::unix::fs::PermissionsExt;
    let counted_tool = folder.path().join("counted-git");
    std::fs::write(
        &counted_tool,
        "#!/bin/sh\nprintf 'call\\n' >> \"$0.count\"\nexec /usr/bin/git \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&counted_tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    let batch_reader = GitObjectReader::new(counted_tool.clone(), Duration::from_secs(10)).unwrap();
    let batch = batch_reader
        .blobs(
            &bare,
            &entries,
            GitObjectFormat::Sha1,
            &SourceLimits::default(),
        )
        .unwrap();
    assert_eq!(batch.len(), entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let expected = objects
            .iter()
            .find(|(name, _, _)| *name == entry.path)
            .unwrap()
            .2;
        assert_eq!(batch.get(index), Some(expected));
    }
    assert_eq!(
        std::fs::read_to_string(counted_tool.with_extension("count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let object = objects
        .iter()
        .find(|(name, _, _)| *name == "hidden.rs")
        .unwrap();
    let many = (0..5000)
        .map(|index| GitTreeEntry {
            path: format!("file{index}.rs"),
            object_id: object.1.clone(),
            kind: GitEntryKind::File,
            size: Some(object.2.len() as u64),
        })
        .collect::<Vec<_>>();
    let large = batch_reader
        .blobs(
            &bare,
            &many,
            GitObjectFormat::Sha1,
            &SourceLimits::default(),
        )
        .unwrap();
    assert_eq!(large.len(), 5000);
    assert_eq!(large.get(4999), Some(object.2));
    assert_eq!(
        std::fs::read_to_string(counted_tool.with_extension("count"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert!(
        batch_reader
            .blobs(&bare, &[], GitObjectFormat::Sha1, &SourceLimits::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        std::fs::read_to_string(counted_tool.with_extension("count"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    for (name, oid, bytes) in objects {
        let entry = entries.iter().find(|entry| entry.path == name).unwrap();
        assert_eq!(entry.object_id, oid);
        assert_eq!(
            reader
                .blob(&bare, &oid, GitObjectFormat::Sha1, bytes.len() as u64, 1024)
                .unwrap(),
            bytes
        );
    }
    assert!(
        !marker.exists(),
        "Reading objects executed a repository filter"
    );
    assert!(
        reader
            .blob(&bare, &entries[0].object_id, GitObjectFormat::Sha1, 0, 1024)
            .is_err()
    );
    assert!(
        reader
            .blob(
                &bare,
                &entries[0].object_id,
                GitObjectFormat::Sha1,
                2048,
                1024
            )
            .is_err()
    );
    assert!(
        reader
            .tree(
                &bare,
                "--help",
                GitObjectFormat::Sha1,
                &SourceLimits::default()
            )
            .is_err()
    );
}
