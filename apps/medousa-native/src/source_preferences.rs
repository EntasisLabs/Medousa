//! Device-local native destinations. These are view preferences, not a daemon
//! activation or replicated content. Persisting them must never move authority.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
};
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub aliases: HashMap<String, String>,
    pub views: HashMap<String, String>,
    pub new_chat: Option<String>,
}
impl Preferences {
    pub fn read(path: &Path) -> Result<Self, String> {
        match std::fs::read(path){
            Ok(raw)=>serde_json::from_slice(&raw).map_err(|_|"Native source preferences are unreadable; they were not overwritten. Select destinations explicitly and repair the preferences file.".into()),
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(Self::default()),
            Err(_)=>Err("Cannot read native source preferences; no destination was substituted.".into()),
        }
    }
}
pub struct Store {
    tx: mpsc::Sender<Preferences>,
    errors: Mutex<mpsc::Receiver<String>>,
}
impl Store {
    pub fn new(path: PathBuf, initial_error: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel::<Preferences>();
        let (errors, updates) = mpsc::channel();
        std::thread::spawn(move || {
            if let Some(error) = initial_error {
                let _ = errors.send(error);
                return;
            }
            while let Ok(mut value) = rx.recv() {
                for newer in rx.try_iter() {
                    value = newer;
                }
                let result = (|| -> Result<(), String> {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    let temp = path.with_extension(format!("native-{}.tmp", std::process::id()));
                    std::fs::write(
                        &temp,
                        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                    std::fs::rename(temp, &path).map_err(|e| e.to_string())
                })();
                if let Err(e) = result {
                    let _ = errors.send(format!("Could not save native view destinations: {e}"));
                }
            }
        });
        Self {
            tx,
            errors: Mutex::new(updates),
        }
    }
    pub fn save(&self, prefs: Preferences) {
        let _ = self.tx.send(prefs);
    }
    pub fn errors(&self) -> Vec<String> {
        self.errors.lock().unwrap().try_iter().collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_destination_is_not_replaced_on_restore() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sources.json");
        let p = Preferences {
            aliases: HashMap::from([
                ("a".into(), "authority-a".into()),
                ("b".into(), "authority-b".into()),
            ]),
            views: HashMap::from([
                ("Notes".into(), "authority-a".into()),
                ("Create".into(), "authority-b".into()),
            ]),
            new_chat: Some("authority-a".into()),
        };
        std::fs::write(&path, serde_json::to_vec(&p).unwrap()).unwrap();
        let restored = Preferences::read(&path).unwrap();
        assert_eq!(restored.views["Notes"], "authority-a");
        assert_eq!(restored.views["Create"], "authority-b");
        assert_eq!(restored.new_chat, p.new_chat);
        std::fs::write(&path, "broken").unwrap();
        assert!(Preferences::read(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
    }
}
