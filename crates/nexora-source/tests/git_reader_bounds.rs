#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/test_directory.rs"]
mod test_directory;
use nexora_source::{GitObjectFormat, GitObjectReader, SourceError, SourceLimits};
use std::{os::unix::fs::PermissionsExt, time::Duration};
use test_directory::TestDirectory;

#[test]
fn bounded_reader_stops_oversized_output_and_times_out_the_entire_owned_process_group() {
    let folder = TestDirectory::new("reader-bounds");
    let tool = folder.path().join("controlled-git");
    std::fs::write(
        &tool,
        "#!/bin/sh\nwhile :; do printf 12345678901234567890; done\n",
    )
    .unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    let reader = GitObjectReader::new(tool.clone(), Duration::from_secs(2)).unwrap();
    let limits = SourceLimits {
        max_tree_bytes: 32,
        ..SourceLimits::default()
    };
    assert!(matches!(
        reader.tree(
            folder.path(),
            &"a".repeat(40),
            GitObjectFormat::Sha1,
            &limits
        ),
        Err(SourceError::LimitExceeded)
    ));
    let child_marker = tool.with_extension("marker");
    let child_started = tool.with_extension("started");
    // This is an operator-tool test fixture, outside every repository tree.
    let script =
        "#!/bin/sh\n(: >\"$0.started\"; /bin/sleep 3; /usr/bin/touch \"$0.marker\") &\nwait\n";
    std::fs::write(&tool, script).unwrap();
    let reader = GitObjectReader::new(tool, Duration::from_secs(2)).unwrap();
    assert!(matches!(
        reader.tree(
            folder.path(),
            &"a".repeat(40),
            GitObjectFormat::Sha1,
            &SourceLimits::default()
        ),
        Err(SourceError::TimedOut)
    ));
    assert!(
        child_started.exists(),
        "Tool did not start its helper before deadline"
    );
    std::thread::sleep(Duration::from_millis(3200));
    assert!(
        !child_marker.exists(),
        "Timed out helper outlived the reader process group"
    );
}
