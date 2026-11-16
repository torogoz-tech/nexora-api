use nexora_source::{GitObjectFormat, GitTreeDecoder, SourceLimits};

#[test]
fn unsafe_ambiguous_malformed_or_over_budget_trees_are_rejected_before_materialization() {
    let oid = "a".repeat(40);
    let limits = SourceLimits::default();
    for path in [
        "../escape",
        "/absolute",
        "a/../escape",
        "a//b",
        "a/./b",
        "a\\b",
        ".git/config",
        "A/.GiT/hooks/run",
        "line\nfeed",
    ] {
        let input = format!("100644 blob {oid} 1\t{path}\0");
        assert!(
            GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &limits).is_err(),
            "Unsafe tree path accepted"
        );
    }
    for input in [
        format!("100644 blob {oid} 1\tfile"),
        "100644 blob not-an-object 1\tfile\0".to_owned(),
        format!("100644 commit {oid} 1\tfile\0"),
        format!("100644 blob {oid} -1\tfile\0"),
        format!("100644 blob {oid} 18446744073709551616\tfile\0"),
        format!(
            "100644 blob {oid} 1\tFile\0\
100644 blob {oid} 1\tfile\0"
        ),
        format!(
            "100644 blob {oid} 1\tFolder/a\0\
100644 blob {oid} 1\tfolder/b\0"
        ),
        format!(
            "100644 blob {oid} 1\tparent\0\
100644 blob {oid} 1\tparent/child\0"
        ),
    ] {
        assert!(
            GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &limits).is_err(),
            "Malformed or ambiguous tree accepted"
        );
    }
    let input = format!(
        "100644 blob {oid} 4\tone\0\
100644 blob {oid} 4\ttwo\0"
    );
    let mut limits = SourceLimits {
        max_total_bytes: 7,
        ..SourceLimits::default()
    };
    assert!(GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &limits).is_err());
    limits.max_total_bytes = 8;
    limits.max_file_bytes = 3;
    assert!(GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &limits).is_err());
    limits.max_file_bytes = 4;
    limits.max_entries = 1;
    assert!(GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &limits).is_err());
    limits.max_entries = 2;
    limits.max_tree_bytes = input.len() - 1;
    assert!(GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &limits).is_err());
    assert!(
        GitTreeDecoder::decode(
            input.as_bytes(),
            GitObjectFormat::Sha256,
            &SourceLimits::default()
        )
        .is_err()
    );
}

#[test]
fn default_tree_limits_accept_files_larger_than_the_old_per_file_ceiling() {
    let size = 40 * 1024 * 1024;
    let input = format!("100644 blob {} {size}\tlarge.js\0", "a".repeat(40));
    let limits = SourceLimits::default();
    assert!(GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &limits).is_ok());
    let mut explicit = limits;
    explicit.max_file_bytes = 32 * 1024 * 1024;
    assert!(GitTreeDecoder::decode(input.as_bytes(), GitObjectFormat::Sha1, &explicit).is_err());
}
