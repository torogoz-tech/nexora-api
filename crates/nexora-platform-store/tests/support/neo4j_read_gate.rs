use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::JoinHandle,
    time::Duration,
};

pub struct Neo4jReadGate {
    child: Child,
    notifications: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    pub port: u16,
}
impl Neo4jReadGate {
    pub fn start(upstream_port: u16) -> Self {
        let mut child = Command::new("python3")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/support/neo4j_read_gate.py"),
            )
            .arg(upstream_port.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, notifications) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut gate = Self {
            child,
            notifications,
            reader: Some(reader),
            port: 0,
        };
        let ready = gate
            .notifications
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        gate.port = ready.strip_prefix("ready ").unwrap().parse().unwrap();
        gate
    }
    pub fn wait_read(&self) -> bool {
        self.notifications
            .recv_timeout(Duration::from_secs(10))
            .is_ok_and(|message| message == "waiting")
    }
    pub fn release(&mut self) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"release\n")
            .unwrap();
        self.child.stdin.as_mut().unwrap().flush().unwrap();
    }
}
impl Drop for Neo4jReadGate {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
