/// Strict decoding of one bounded source. Original byte hashes retain provenance
/// for transcoded or omitted sources without fabricating replacement characters.
pub enum SourceReadOutcome {
    Decoded {
        content: String,
        encoding: &'static str,
        checksum: String,
        raw_bytes: usize,
    },
    Skipped {
        reason: String,
        checksum: String,
        raw_bytes: usize,
    },
}
