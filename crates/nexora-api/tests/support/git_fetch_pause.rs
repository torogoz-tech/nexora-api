use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct GitFetchPause {
    pause: PathBuf,
    waiting: PathBuf,
}
impl GitFetchPause {
    pub fn new(control: &Path) -> Self {
        let pause = control.with_extension("git-pause");
        let waiting = control.with_extension("git-waiting");
        let _ = std::fs::remove_file(&waiting);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&pause).unwrap();
        Self { pause, waiting }
    }
    pub fn wait(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self.waiting.is_file() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }
    pub fn release(&self) {
        let _ = std::fs::remove_file(&self.pause);
    }
}
impl Drop for GitFetchPause {
    fn drop(&mut self) {
        self.release();
    }
}
