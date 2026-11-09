//! Настройки, недавние проекты и безопасное хранение токена (Диспетчер учётных данных Windows).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub anim: u8,
    pub login: Option<String>,
    pub name: Option<String>,
    pub auth_method: String,
    pub recents: Vec<String>,
    pub default_private: bool,
    pub default_branch: String,
    pub first_run_done: bool,
    pub auto_open_browser: bool,
    pub scan_secrets: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            anim: 2,
            login: None,
            name: None,
            auth_method: String::new(),
            recents: vec![],
            default_private: true,
            default_branch: "main".into(),
            first_run_done: false,
            auto_open_browser: true,
            scan_secrets: true,
        }
    }
}

fn dir() -> PathBuf {
    let d = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("GitHelper");
    let _ = std::fs::create_dir_all(&d);
    d
}

impl Config {
    pub fn load() -> Config {
        std::fs::read_to_string(dir().join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir().join("config.json"), s);
        }
    }

    pub fn push_recent(&mut self, path: &str) {
        self.recents.retain(|p| !p.eq_ignore_ascii_case(path));
        self.recents.insert(0, path.to_string());
        self.recents.truncate(10);
        self.save();
    }
}

const SERVICE: &str = "GitHelper";
const ACCOUNT: &str = "github-token";

pub fn save_token(token: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, ACCOUNT)
        .and_then(|e| e.set_password(token))
        .map_err(|e| e.to_string())
}

pub fn load_token() -> Option<String> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()?.get_password().ok()
}

pub fn delete_token() {
    if let Ok(e) = keyring::Entry::new(SERVICE, ACCOUNT) {
        let _ = e.delete_credential();
    }
}
