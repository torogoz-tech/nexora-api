use nexora_source::{GitBatchDecoder, GitEntryKind, GitObjectFormat, GitTreeEntry, SourceLimits};

#[test]
fn batch_frames_validate_identity_type_size_delimiters_and_total_before_use() {
    let oid = "a".repeat(40);
    let entries = vec![
        GitTreeEntry {
            path: "binary.dat".into(),
            object_id: oid.clone(),
            kind: GitEntryKind::File,
            size: Some(4),
        },
        GitTreeEntry {
            path: "empty.rs".into(),
            object_id: "b".repeat(40),
            kind: GitEntryKind::File,
            size: Some(0),
        },
    ];
    let mut raw = format!("{oid} blob 4\0").into_bytes();
    raw.extend_from_slice(b"a\0b\n\0");
    raw.extend_from_slice(format!("{} blob 0\0\0", entries[1].object_id).as_bytes());
    let batch = GitBatchDecoder::decode(
        raw.clone(),
        &entries,
        GitObjectFormat::Sha1,
        &SourceLimits::default(),
    )
    .unwrap();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch.get(0), Some(b"a\0b\n".as_slice()));
    assert_eq!(batch.get(1), Some(b"".as_slice()));
    assert!(batch.get(2).is_none());
    for invalid in [
        format!("{} blob 4\0a\0b\n\0", "c".repeat(40)).into_bytes(),
        format!("{oid} commit 4\0a\0b\n\0").into_bytes(),
        format!("{oid} blob 5\0a\0b\n\0").into_bytes(),
        raw[..raw.len() - 1].to_vec(),
        [raw.as_slice(), b"trailing"].concat(),
    ] {
        assert!(
            GitBatchDecoder::decode(
                invalid,
                &entries,
                GitObjectFormat::Sha1,
                &SourceLimits::default()
            )
            .is_err()
        );
    }
    let limits = SourceLimits {
        max_total_bytes: 3,
        ..SourceLimits::default()
    };
    assert!(GitBatchDecoder::decode(raw, &entries, GitObjectFormat::Sha1, &limits).is_err());
}
