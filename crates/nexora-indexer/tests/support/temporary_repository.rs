use std::{fs, path::PathBuf};
pub struct TemporaryRepository {
    pub path: PathBuf,
}
impl TemporaryRepository {
    pub fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "nexora-module-fixture-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        for (name, source) in files {
            let file = path.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, source).unwrap();
        }
        Self { path }
    }
}
impl Drop for TemporaryRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
