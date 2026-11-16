use super::git_fixture::GitFixture;
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
};
pub struct GitHttpFixture {
    child: Child,
    origin: String,
    token: String,
    stats: PathBuf,
}
impl GitHttpFixture {
    pub fn start(fixture: &GitFixture) -> Self {
        let source = fixture.directory.path().join("server/team/source.git");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        assert!(
            Command::new("/bin/cp")
                .args(["-R"])
                .arg(&fixture.bare)
                .arg(&source)
                .output()
                .unwrap()
                .status
                .success()
        );
        let settings = fixture.directory.path().join("http-settings.json");
        let stats = fixture.directory.path().join("http-stats.json");
        let mut child = Command::new("python3")
            .arg("tests/support/git_http_server.py")
            .arg(fixture.directory.path().join("server"))
            .arg(&settings)
            .arg(&stats)
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
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(settings).unwrap()).unwrap();
        Self {
            child,
            origin: value["origin"].as_str().unwrap().into(),
            token: value["token"].as_str().unwrap().into(),
            stats,
        }
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn trap_requests(&self) -> u64 {
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&self.stats).unwrap()).unwrap()["trap"].as_u64().unwrap()
    }
}
impl Drop for GitHttpFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
