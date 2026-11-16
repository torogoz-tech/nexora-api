use super::git_fixture::GitFixture;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
};
use zeroize::Zeroizing;

pub struct GitlabSourceFixture {
    child: Child,
    origin: String,
    token: Zeroizing<String>,
    control: PathBuf,
}
impl GitlabSourceFixture {
    pub fn start(fixture: &GitFixture, commit: &str) -> Self {
        let source = fixture.directory.path().join("provider/fixture/source.git");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        assert!(
            Command::new("/bin/cp")
                .arg("-R")
                .arg(&fixture.bare)
                .arg(&source)
                .output()
                .unwrap()
                .status
                .success()
        );
        let settings = fixture.directory.path().join("provider-settings.json");
        let control = fixture.directory.path().join("provider-control.json");
        let mut child = Command::new("python3")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../nexora-platform-store/tests/support/gitlab_source_server.py"),
            )
            .arg(fixture.directory.path().join("provider"))
            .arg(&settings)
            .arg(&control)
            .arg(commit)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        let value: Value = serde_json::from_slice(&std::fs::read(settings).unwrap()).unwrap();
        Self {
            child,
            origin: value["origin"].as_str().unwrap().into(),
            token: Zeroizing::new(value["token"].as_str().unwrap().into()),
            control,
        }
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn control(&self, value: Value) {
        std::fs::write(&self.control, serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
impl Drop for GitlabSourceFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
