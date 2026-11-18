// Consumers include only the fixture operations needed by their scenario.
#![allow(dead_code)]

use super::test_directory::TestDirectory;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
pub struct GitFixture {
    pub directory: TestDirectory,
    pub bare: PathBuf,
}
impl GitFixture {
    pub fn new(label: &str) -> Self {
        let directory = TestDirectory::new(label);
        let bare = directory.path().join("fixture.git");
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
        Self { directory, bare }
    }
    pub fn run(&self, args: &[&str], input: &[u8]) -> Vec<u8> {
        let mut child = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(&self.bare)
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
        child.stdin.take().unwrap().write_all(input).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success(), "Controlled fixture setup failed");
        result.stdout
    }
    pub fn blob(&self, bytes: &[u8]) -> String {
        String::from_utf8(self.run(&["hash-object", "-w", "--stdin", "--no-filters"], bytes))
            .unwrap()
            .trim()
            .to_owned()
    }
    pub fn tree(&self, entries: &[(&str, &str, &str)]) -> String {
        let input = entries
            .iter()
            .map(|(mode, oid, name)| {
                let kind = if *mode == "040000" {
                    "tree"
                } else if *mode == "160000" {
                    "commit"
                } else {
                    "blob"
                };
                format!("{mode} {kind} {oid}\t{name}\0")
            })
            .collect::<String>();
        String::from_utf8(self.run(&["mktree", "--missing", "-z"], input.as_bytes()))
            .unwrap()
            .trim()
            .to_owned()
    }
    pub fn commit(&self, tree: &str) -> String {
        String::from_utf8(self.run(&["commit-tree", tree, "-m", "Fixture"], b""))
            .unwrap()
            .trim()
            .to_owned()
    }
}
