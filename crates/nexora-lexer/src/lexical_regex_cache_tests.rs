use crate::lexical_regex_cache::LexicalRegexCache;
#[test]
fn anchored_patterns_are_shared_without_changing_flags_or_caching_failures() {
    let mut cache = LexicalRegexCache::default();
    let first = cache.compile("ab").unwrap();
    let second = cache.compile("ab").unwrap();
    assert!(first.is_match("abc") && second.is_match("ab"));
    assert!(!second.is_match("zab"));
    assert_eq!(cache.compiled_patterns(), 1);
    let folded = cache.compile("(?i:ab)").unwrap();
    assert!(folded.is_match("AB"));
    assert!(!first.is_match("AB"));
    assert_eq!(cache.compiled_patterns(), 2);
    assert!(cache.compile("(").is_err());
    assert_eq!(cache.compiled_patterns(), 2);
}
#[test]
fn distinct_cache_entries_are_bounded_but_reuse_at_capacity_is_allowed() {
    let mut cache = LexicalRegexCache::default();
    for i in 0..512 {
        cache.compile(&format!("word{i}")).unwrap();
    }
    assert_eq!(cache.compiled_patterns(), 512);
    assert!(cache.compile("word0").unwrap().is_match("word0"));
    assert!(cache.compile("extra").is_err());
    assert_eq!(cache.compiled_patterns(), 512);
}
