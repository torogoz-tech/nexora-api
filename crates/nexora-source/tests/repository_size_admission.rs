use nexora_source::{GitObjectFormat, GitTransferLimits, GitTreeDecoder, SourceLimits};

#[test]
fn production_defaults_use_finite_repository_and_file_quotas() {
    let oid = "a".repeat(40);
    let mut tree = format!("100644 blob {oid} 314572800\tlarge.js\0");
    for index in 0..20_001 {
        tree.push_str(&format!("100644 blob {oid} 1\tfile-{index}.js\0"));
    }
    let entries = GitTreeDecoder::decode(
        tree.as_bytes(),
        GitObjectFormat::Sha1,
        &SourceLimits::default(),
    )
    .unwrap();
    assert_eq!(entries.len(), 20_002);
    let transfer = GitTransferLimits::default();
    assert_eq!(transfer.max_pack_bytes, 256 * 1024 * 1024);
    assert_eq!(transfer.max_repository_bytes, 2 * 1024 * 1024 * 1024);
    assert_eq!(transfer.max_repository_entries, 250_000);
    transfer.validate().unwrap();
}
