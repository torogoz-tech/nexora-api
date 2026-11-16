use nexora_source::{GitEntryKind, GitObjectFormat, GitTreeDecoder, SourceLimits};

#[test]
fn raw_git_tree_preserves_names_object_ids_and_special_entry_metadata() {
    let oid = "a".repeat(40);
    let input = format!(
        "100644 blob {oid}       7\tsrc/name with spaces.rs\0\
100755 blob {oid}      13\ttools/unexecuted.sh\0\
120000 blob {oid}      12\tlink\0\
160000 commit {oid}       -\tdependency\0"
    );
    let entries = GitTreeDecoder::decode(
        input.as_bytes(),
        GitObjectFormat::Sha1,
        &SourceLimits::default(),
    )
    .unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].path, "src/name with spaces.rs");
    assert_eq!(entries[0].object_id, oid);
    assert_eq!(entries[0].kind, GitEntryKind::File);
    assert_eq!(entries[0].size, Some(7));
    assert_eq!(entries[1].kind, GitEntryKind::ExecutableFile);
    assert_eq!(entries[2].kind, GitEntryKind::SymbolicLink);
    assert_eq!(entries[3].kind, GitEntryKind::Submodule);
    assert_eq!(entries[3].size, None);
    let sha256 = format!("100644 blob {} 0\tUnicode/niño.cs\0", "b".repeat(64));
    assert_eq!(
        GitTreeDecoder::decode(
            sha256.as_bytes(),
            GitObjectFormat::Sha256,
            &SourceLimits::default()
        )
        .unwrap()[0]
            .path,
        "Unicode/niño.cs"
    );
}
