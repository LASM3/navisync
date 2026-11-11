use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn default_dir() -> PathBuf {
        PathBuf::from("cache")
    }

    pub fn entry_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    pub fn load(&self, key: &str) -> Option<String> {
        fs::read_to_string(self.entry_path(key)).ok()
    }

    pub fn save(&self, key: &str, body: &str) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let _ = fs::write(self.entry_path(key), body);
    }

    pub fn clear(&self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
