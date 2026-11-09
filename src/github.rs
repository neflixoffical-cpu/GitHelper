//! Клиент GitHub REST API.

use serde_json::{json, Value};

const API: &str = "https://api.github.com";
const UA: &str = "GitHelper-CLI";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(25)).build()
}

fn req(method: &str, path: &str, token: Option<&str>) -> ureq::Request {
    let mut r = agent()
        .request(method, &format!("{}{}", API, path))
        .set("User-Agent", UA)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = token {
        r = r.set("Authorization", &format!("Bearer {}", t));
    }
    r
}

fn explain(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: Value = resp.into_json().unwrap_or(Value::Null);
            let mut msg = body["message"].as_str().unwrap_or("").to_string();
            if let Some(errs) = body["errors"].as_array() {
                for e in errs {
                    if let Some(m) = e["message"].as_str() {
                        msg.push_str(&format!(" — {}", m));
                    }
                }
            }
            let hint = match code {
                401 => "токен неверный или истёк",
                403 => "недостаточно прав у токена или превышен лимит запросов",
                404 => "не найдено (или нет доступа)",
                422 => "некорректные данные (возможно, такой репозиторий уже есть)",
                _ => "",
            };
            format!("HTTP {}: {} {}", code, hint, msg).trim().to_string()
        }
        ureq::Error::Transport(t) => format!("Нет соединения с GitHub: {}", t),
    }
}

pub struct User {
    pub login: String,
    pub name: String,
    pub scopes: Option<String>,
    pub public_repos: u64,
    pub private_repos: u64,
    pub email: Option<String>,
}

pub fn user(token: &str) -> Result<User, String> {
    let r = req("GET", "/user", Some(token)).call().map_err(explain)?;
    let scopes = r.header("X-OAuth-Scopes").map(|s| s.to_string());
    let v: Value = r.into_json().map_err(|e| e.to_string())?;
    Ok(User {
        login: v["login"].as_str().unwrap_or("").into(),
        name: v["name"].as_str().unwrap_or("").into(),
        scopes,
        public_repos: v["public_repos"].as_u64().unwrap_or(0),
        private_repos: v["total_private_repos"].as_u64().unwrap_or(0),
        email: v["email"].as_str().map(|s| s.to_string()),
    })
}

/// Приватный noreply-адрес для коммитов, если почта скрыта.
pub fn noreply_email(token: &str, login: &str) -> String {
    let id = req("GET", "/user", Some(token))
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok())
        .and_then(|v| v["id"].as_u64());
    match id {
        Some(id) => format!("{}+{}@users.noreply.github.com", id, login),
        None => format!("{}@users.noreply.github.com", login),
    }
}

#[allow(dead_code)]
pub struct Repo {
    pub full_name: String,
    pub clone_url: String,
    pub html_url: String,
    pub private: bool,
    pub description: String,
    pub stars: u64,
    pub updated: String,
    pub language: String,
}

fn to_repo(v: &Value) -> Repo {
    Repo {
        full_name: v["full_name"].as_str().unwrap_or("").into(),
        clone_url: v["clone_url"].as_str().unwrap_or("").into(),
        html_url: v["html_url"].as_str().unwrap_or("").into(),
        private: v["private"].as_bool().unwrap_or(false),
        description: v["description"].as_str().unwrap_or("").into(),
        stars: v["stargazers_count"].as_u64().unwrap_or(0),
        updated: v["pushed_at"].as_str().unwrap_or("").chars().take(10).collect(),
        language: v["language"].as_str().unwrap_or("").into(),
    }
}

pub fn list_repos(token: &str) -> Result<Vec<Repo>, String> {
    let mut all = vec![];
    for page in 1..=5 {
        let r = req("GET", &format!("/user/repos?per_page=100&sort=pushed&page={}", page), Some(token))
            .call()
            .map_err(explain)?;
        let v: Value = r.into_json().map_err(|e| e.to_string())?;
        let arr = v.as_array().cloned().unwrap_or_default();
        let n = arr.len();
        all.extend(arr.iter().map(to_repo));
        if n < 100 {
            break;
        }
    }
    Ok(all)
}

pub fn get_repo(token: &str, owner: &str, name: &str) -> Result<Repo, String> {
    let r = req("GET", &format!("/repos/{}/{}", owner, name), Some(token)).call().map_err(explain)?;
    let v: Value = r.into_json().map_err(|e| e.to_string())?;
    Ok(to_repo(&v))
}

pub fn create_repo(token: &str, name: &str, desc: &str, private: bool) -> Result<Repo, String> {
    let r = req("POST", "/user/repos", Some(token))
        .send_json(json!({ "name": name, "description": desc, "private": private, "auto_init": false }))
        .map_err(explain)?;
    let v: Value = r.into_json().map_err(|e| e.to_string())?;
    Ok(to_repo(&v))
}

pub fn gitignore_list() -> Result<Vec<String>, String> {
    let r = req("GET", "/gitignore/templates", None).call().map_err(explain)?;
    let v: Vec<String> = r.into_json().map_err(|e| e.to_string())?;
    Ok(v)
}

pub fn gitignore_get(name: &str) -> Result<String, String> {
    let r = req("GET", &format!("/gitignore/templates/{}", name), None).call().map_err(explain)?;
    let v: Value = r.into_json().map_err(|e| e.to_string())?;
    Ok(v["source"].as_str().unwrap_or("").to_string())
}

pub fn license_get(key: &str) -> Result<String, String> {
    let r = req("GET", &format!("/licenses/{}", key), None).call().map_err(explain)?;
    let v: Value = r.into_json().map_err(|e| e.to_string())?;
    Ok(v["body"].as_str().unwrap_or("").to_string())
}

pub fn create_release(
    token: &str,
    owner: &str,
    repo: &str,
    tag: &str,
    name: &str,
    body: &str,
    prerelease: bool,
) -> Result<String, String> {
    let r = req("POST", &format!("/repos/{}/{}/releases", owner, repo), Some(token))
        .send_json(json!({ "tag_name": tag, "name": name, "body": body, "prerelease": prerelease, "generate_release_notes": body.is_empty() }))
        .map_err(explain)?;
    let v: Value = r.into_json().map_err(|e| e.to_string())?;
    Ok(v["html_url"].as_str().unwrap_or("").to_string())
}

pub fn rate_limit(token: Option<&str>) -> Option<(u64, u64)> {
    let v: Value = req("GET", "/rate_limit", token).call().ok()?.into_json().ok()?;
    Some((v["rate"]["remaining"].as_u64()?, v["rate"]["limit"].as_u64()?))
}

pub fn ping() -> bool {
    req("GET", "/zen", None).call().is_ok()
}
