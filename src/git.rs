//! Обёртка над git.exe.

use base64::Engine;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static GIT: Mutex<String> = Mutex::new(String::new());

fn bin() -> String {
    let g = GIT.lock().unwrap();
    if g.is_empty() {
        "git".into()
    } else {
        g.clone()
    }
}

fn cmd(dir: Option<&Path>) -> Command {
    let mut c = Command::new(bin());
    if let Some(d) = dir {
        c.current_dir(d);
    }
    c.env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C.UTF-8")
        .args(["-c", "core.quotepath=off", "-c", "color.ui=never"]);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Ищет git: в PATH, затем в стандартных папках установки (важно сразу после установки через winget).
pub fn detect() -> Option<String> {
    let try_bin = |b: &str| -> Option<String> {
        let mut c = Command::new(b);
        c.arg("--version");
        #[cfg(windows)]
        c.creation_flags(CREATE_NO_WINDOW);
        let o = c.output().ok()?;
        if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).trim().replace("git version ", ""))
        } else {
            None
        }
    };
    if let Some(v) = try_bin("git") {
        *GIT.lock().unwrap() = "git".into();
        return Some(v);
    }
    let mut candidates = vec![
        r"C:\Program Files\Git\cmd\git.exe".to_string(),
        r"C:\Program Files (x86)\Git\cmd\git.exe".to_string(),
    ];
    if let Some(l) = dirs::data_local_dir() {
        candidates.push(l.join(r"Programs\Git\cmd\git.exe").to_string_lossy().into_owned());
    }
    for cnd in candidates {
        if Path::new(&cnd).exists() {
            if let Some(v) = try_bin(&cnd) {
                *GIT.lock().unwrap() = cnd;
                return Some(v);
            }
        }
    }
    None
}

pub fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = cmd(Some(dir)).args(args).output().map_err(|e| e.to_string())?;
    let so = String::from_utf8_lossy(&o.stdout).to_string();
    let se = String::from_utf8_lossy(&o.stderr).to_string();
    if o.status.success() {
        Ok(so.trim_end().to_string())
    } else {
        let msg = if se.trim().is_empty() { so } else { se };
        Err(msg.trim().to_string())
    }
}

pub fn run_global(args: &[&str]) -> Result<String, String> {
    let o = cmd(None).args(args).output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// Сетевые команды (push/pull/fetch/clone) с токеном: токен передаётся заголовком
/// и нигде не сохраняется — ни в .git/config, ни в URL remote.
pub fn run_net(dir: &Path, args: &[&str], auth: Option<(&str, &str)>) -> Result<String, String> {
    let mut c = cmd(Some(dir));
    if let Some((login, token)) = auth {
        let b = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", login, token));
        c.args([
            "-c",
            "credential.helper=",
            "-c",
            &format!("http.https://github.com/.extraheader=AUTHORIZATION: basic {}", b),
        ]);
    }
    let o = c.args(args).output().map_err(|e| e.to_string())?;
    let so = String::from_utf8_lossy(&o.stdout).to_string();
    let se = String::from_utf8_lossy(&o.stderr).to_string();
    if o.status.success() {
        Ok(format!("{}{}", so, se).trim().to_string())
    } else {
        Err(se.trim().to_string())
    }
}

/// Вход через Git Credential Manager: открывает окно входа GitHub
/// (почта + пароль + 2FA в браузере) и возвращает выданный OAuth-токен.
pub fn credential_fill() -> Result<(String, String), String> {
    let mut child = cmd(None)
        .args(["credential", "fill"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"protocol=https\nhost=github.com\n\n")
        .map_err(|e| e.to_string())?;
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    let s = String::from_utf8_lossy(&o.stdout);
    let mut user = String::new();
    let mut pass = String::new();
    for l in s.lines() {
        if let Some(v) = l.strip_prefix("username=") {
            user = v.to_string();
        }
        if let Some(v) = l.strip_prefix("password=") {
            pass = v.to_string();
        }
    }
    if pass.is_empty() {
        Err("Менеджер учётных данных не вернул токен".into())
    } else {
        Ok((user, pass))
    }
}

pub fn credential_approve(user: &str, pass: &str) {
    credential_op("approve", user, pass);
}
pub fn credential_reject(user: &str, pass: &str) {
    credential_op("reject", user, pass);
}
fn credential_op(op: &str, user: &str, pass: &str) {
    if let Ok(mut child) = cmd(None)
        .args(["credential", op])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        let data = format!("protocol=https\nhost=github.com\nusername={}\npassword={}\n\n", user, pass);
        let _ = child.stdin.take().unwrap().write_all(data.as_bytes());
        let _ = child.wait();
    }
}

pub fn has_gcm() -> bool {
    run_global(&["credential-manager", "--version"]).is_ok()
}

// ───────────── состояние репозитория ─────────────

pub fn is_repo(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--is-inside-work-tree"]).map(|s| s == "true").unwrap_or(false)
}

pub fn toplevel(dir: &Path) -> Option<String> {
    run(dir, &["rev-parse", "--show-toplevel"]).ok()
}

pub fn branch(dir: &Path) -> String {
    run(dir, &["branch", "--show-current"]).unwrap_or_default()
}

pub fn has_commits(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--verify", "HEAD"]).is_ok()
}

pub fn remote_url(dir: &Path) -> Option<String> {
    run(dir, &["remote", "get-url", "origin"]).ok().filter(|s| !s.is_empty())
}

pub fn config_get(key: &str) -> Option<String> {
    run_global(&["config", "--global", key]).ok().filter(|s| !s.is_empty())
}

/// owner/repo из URL remote GitHub.
pub fn github_slug(url: &str) -> Option<(String, String)> {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = u
        .strip_prefix("https://github.com/")
        .or_else(|| u.strip_prefix("http://github.com/"))
        .or_else(|| u.strip_prefix("git@github.com:"))
        .or_else(|| u.strip_prefix("ssh://git@github.com/"))?;
    let rest = rest.rsplit('@').next().unwrap_or(rest);
    let mut p = rest.split('/');
    Some((p.next()?.to_string(), p.next()?.to_string()))
}

#[derive(Clone)]
pub struct Change {
    pub x: char,
    pub y: char,
    pub path: String,
}

impl Change {
    pub fn kind(&self) -> &'static str {
        match (self.x, self.y) {
            ('?', '?') => "новый",
            ('A', _) => "добавлен",
            ('D', _) | (_, 'D') => "удалён",
            ('R', _) => "переименован",
            ('U', _) | (_, 'U') => "конфликт",
            _ => "изменён",
        }
    }
    #[allow(dead_code)]
    pub fn staged(&self) -> bool {
        self.x != ' ' && self.x != '?'
    }
}

pub fn status(dir: &Path) -> Vec<Change> {
    let s = match run(dir, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let mut res = vec![];
    let mut parts = s.split('\0');
    while let Some(p) = parts.next() {
        if p.len() < 4 {
            continue;
        }
        let mut ch = p.chars();
        let x = ch.next().unwrap();
        let y = ch.next().unwrap();
        let path = p[3..].to_string();
        if x == 'R' || x == 'C' {
            parts.next(); // старое имя
        }
        res.push(Change { x, y, path });
    }
    res
}

pub fn ahead_behind(dir: &Path) -> Option<(u32, u32)> {
    let s = run(dir, &["rev-list", "--left-right", "--count", "@{u}...HEAD"]).ok()?;
    let mut it = s.split_whitespace();
    let behind = it.next()?.parse().ok()?;
    let ahead = it.next()?.parse().ok()?;
    Some((ahead, behind))
}

pub fn last_commit(dir: &Path) -> Option<String> {
    run(dir, &["log", "-1", "--format=%h · %s · %cr"]).ok().filter(|s| !s.is_empty())
}
