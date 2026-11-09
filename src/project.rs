//! Проект: выбор папки, статус, коммит, публикация, синхронизация, проверка безопасности.

use crate::auth;
use crate::git::{self, Change};
use crate::github;
use crate::gitignore::{self, human};
use crate::ui::{self, c, fg, Check, Item, CYAN, DIM, ERR, FAINT, ICE, OK, PRIMARY, WARN, WHITE};
use crate::App;
use regex::Regex;
use std::path::{Path, PathBuf};

// ───────────────────── выбор папки ─────────────────────

pub fn select_folder(app: &mut App) -> bool {
    ui::header("Выбор папки проекта");
    let mut items = vec![
        Item::new("▣", "Выбрать в проводнике…", "стандартное окно Windows"),
        Item::new("⌨", "Ввести путь вручную", ""),
    ];
    let cwd = std::env::current_dir().unwrap_or_default();
    items.push(Item::new("·", "Текущая папка", &ui::truncate(&cwd.to_string_lossy(), 50)));
    let recents: Vec<String> = app.cfg.recents.iter().filter(|p| Path::new(p).is_dir()).cloned().collect();
    if !recents.is_empty() {
        items.push(Item::sep("Недавние"));
        for r in &recents {
            let p = Path::new(r);
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or(r.clone());
            let tag = if p.join(".git").exists() { "git" } else { "" };
            items.push(Item::new("◷", &name, &format!("{} {}", tag, ui::truncate(r, 50))));
        }
    }
    let base = items.len() - recents.len();
    let chosen: PathBuf = match ui::menu(&items, 0) {
        Some(0) => {
            let start = app.project.clone().unwrap_or(cwd);
            match rfd::FileDialog::new().set_title("GitHelper — выбери папку проекта").set_directory(&start).pick_folder() {
                Some(p) => p,
                None => return false,
            }
        }
        Some(1) => {
            ui::blank();
            match ui::input("Путь к папке", "", false) {
                Some(p) if !p.is_empty() => PathBuf::from(p.trim_matches('"')),
                _ => return false,
            }
        }
        Some(2) => cwd,
        Some(i) if i >= base => PathBuf::from(&recents[i - base]),
        _ => return false,
    };
    set_project(app, chosen)
}

pub fn set_project(app: &mut App, p: PathBuf) -> bool {
    if !p.is_dir() {
        ui::err("Такой папки не существует");
        ui::pause();
        return false;
    }
    let mut p = p.canonicalize().unwrap_or(p);
    // убираем \\?\ префикс Windows
    let s = p.to_string_lossy().to_string();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        p = PathBuf::from(stripped);
    }
    if !p.join(".git").exists() {
        if let Some(top) = git::toplevel(&p) {
            let top = PathBuf::from(top.replace('/', "\\"));
            if top != p {
                ui::blank();
                ui::info(&format!("Эта папка внутри репозитория {}", c(CYAN, &top.to_string_lossy())));
                if ui::confirm("Открыть корень репозитория?", true) {
                    p = top;
                }
            }
        }
    }
    app.cfg.push_recent(&p.to_string_lossy());
    app.project = Some(p);
    true
}

pub fn need_project(app: &mut App) -> Option<PathBuf> {
    if app.project.is_none() && !select_folder(app) {
        return None;
    }
    app.project.clone()
}

pub fn need_repo(app: &mut App) -> Option<PathBuf> {
    let d = need_project(app)?;
    if git::is_repo(&d) {
        return Some(d);
    }
    ui::header("Репозиторий");
    ui::warn("В этой папке ещё нет git-репозитория.");
    if init_repo(app, &d) {
        Some(d)
    } else {
        None
    }
}

fn init_repo(app: &App, d: &Path) -> bool {
    if !ui::confirm("Создать репозиторий (git init)?", true) {
        return false;
    }
    let b = app.cfg.default_branch.clone();
    match ui::spin("Создаю репозиторий", || git::run(d, &["init", "-b", &b])) {
        Ok(_) => {
            ui::success(&format!("Репозиторий создан, основная ветка — {}", b));
            true
        }
        Err(e) => {
            ui::err(&e);
            ui::pause();
            false
        }
    }
}

fn ensure_identity(app: &App) -> bool {
    if git::config_get("user.name").is_some() && git::config_get("user.email").is_some() {
        return true;
    }
    ui::blank();
    ui::warn("Git не знает твоё имя и почту — без них коммит невозможен.");
    if app.token.is_some() {
        auth::offer_identity(app, &None);
        if git::config_get("user.email").is_some() {
            return true;
        }
    }
    let name = match ui::input_req("Имя для коммитов", app.cfg.name.as_deref().unwrap_or("")) {
        Some(n) => n,
        None => return false,
    };
    let mail = match ui::input_req("Почта для коммитов", "") {
        Some(m) => m,
        None => return false,
    };
    let _ = git::run_global(&["config", "--global", "user.name", &name]);
    let _ = git::run_global(&["config", "--global", "user.email", &mail]);
    ui::success("Сохранено в глобальных настройках git");
    true
}

// ───────────────────── статус ─────────────────────

fn change_color(ch: &Change) -> ui::Rgb {
    match ch.kind() {
        "новый" | "добавлен" => OK,
        "удалён" => ERR,
        "конфликт" => WARN,
        _ => ICE,
    }
}

pub fn status_screen(app: &mut App) {
    let Some(d) = need_project(app) else { return };
    loop {
        ui::header("Статус проекта");
        let info = ui::spin("Собираю информацию", || {
            let repo = git::is_repo(&d);
            let ch = if repo { git::status(&d) } else { vec![] };
            (repo, git::branch(&d), git::remote_url(&d), git::ahead_behind(&d), git::last_commit(&d), ch)
        });
        let (repo, br, remote, ab, last, changes) = info;
        ui::kv("Папка", &d.to_string_lossy());
        if !repo {
            ui::kv("Git", &c(WARN, "не инициализирован"));
            ui::blank();
            if ui::confirm("Создать репозиторий?", true) {
                init_repo(app, &d);
            }
            ui::pause();
            return;
        }
        ui::kv("Ветка", &c(CYAN, &format!("⎇ {}", br)));
        ui::kv("Remote", &remote.clone().unwrap_or_else(|| c(DIM, "не подключён")));
        if let Some((a, b)) = ab {
            let s = if a == 0 && b == 0 {
                c(OK, "синхронизировано ✔")
            } else {
                format!("{}  {}", c(if a > 0 { WARN } else { DIM }, &format!("↑ {} к отправке", a)), c(if b > 0 { WARN } else { DIM }, &format!("↓ {} к получению", b)))
            };
            ui::kv("Синхронизация", &s);
        }
        ui::kv("Последний коммит", &last.unwrap_or_else(|| c(DIM, "коммитов ещё нет")));
        ui::kv(".gitignore", if d.join(".gitignore").exists() { "есть" } else { "нет" });
        ui::blank();
        if changes.is_empty() {
            ui::success("Рабочая папка чистая — нечего коммитить");
        } else {
            let n = |k: &str| changes.iter().filter(|c| c.kind() == k).count();
            ui::line(&format!(
                "  {}Изменения:  {}  {}  {}  {}",
                fg(WHITE),
                c(ICE, &format!("✎ {} изменено", n("изменён"))),
                c(OK, &format!("＋ {} новых", n("новый") + n("добавлен"))),
                c(ERR, &format!("－ {} удалено", n("удалён"))),
                c(WARN, &format!("⚠ {} конфликтов", n("конфликт")))
            ));
            ui::blank();
            for ch in changes.iter().take(12) {
                ui::line(&format!("    {}{:<12}{} {}", fg(change_color(ch)), ch.kind(), fg(PRIMARY), ch.path));
            }
            if changes.len() > 12 {
                ui::dim(&format!("    … и ещё {}", changes.len() - 12));
            }
        }
        ui::blank();
        let items = vec![
            Item::new("±", "Показать diff", "что именно изменилось"),
            Item::new("✍", "Сделать коммит", ""),
            Item::new("↻", "Обновить", ""),
        ];
        match ui::menu(&items, 0) {
            Some(0) => show_diff(&d),
            Some(1) => {
                commit(app, &d);
            }
            Some(2) => {}
            _ => return,
        }
    }
}

pub fn colorize_diff(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            if l.starts_with("+++") || l.starts_with("---") {
                c(WHITE, l)
            } else if l.starts_with('+') {
                c(OK, l)
            } else if l.starts_with('-') {
                c(ERR, l)
            } else if l.starts_with("@@") {
                c(CYAN, l)
            } else if l.starts_with("diff ") || l.starts_with("index ") {
                c(FAINT, l)
            } else {
                c(DIM, l)
            }
        })
        .collect()
}

fn show_diff(d: &Path) {
    let txt = ui::spin("Считаю изменения", || {
        let mut s = git::run(d, &["diff", "HEAD", "--stat"]).unwrap_or_default();
        s.push_str("\n\n");
        s.push_str(&git::run(d, &["diff", "HEAD"]).or_else(|_| git::run(d, &["diff", "--cached"])).unwrap_or_default());
        s
    });
    if txt.trim().is_empty() {
        ui::info("Отслеживаемые файлы не изменены (новые файлы появятся в diff после коммита).");
        ui::pause();
        return;
    }
    ui::pager("Diff", &colorize_diff(&txt));
}

// ───────────────────── проверка безопасности ─────────────────────

pub struct Finding {
    pub path: String,
    pub what: String,
    pub line: usize,
}

fn patterns() -> Vec<(&'static str, Regex)> {
    let p = [
        ("GitHub токен", r"(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{50,}"),
        ("AWS ключ", r"AKIA[0-9A-Z]{16}"),
        ("Приватный ключ", r"-----BEGIN ([A-Z]+ )?PRIVATE KEY-----"),
        ("Google API ключ", r"AIza[0-9A-Za-z\-_]{35}"),
        ("Slack токен", r"xox[baprs]-[0-9A-Za-z-]{10,}"),
        ("Stripe ключ", r"sk_live_[0-9a-zA-Z]{24,}"),
        ("Telegram бот-токен", r"\b\d{8,10}:[A-Za-z0-9_-]{35}\b"),
        ("Discord бот-токен", r"\b[MNO][A-Za-z\d]{23,27}\.[\w-]{6}\.[\w-]{27,40}\b"),
        ("OpenAI/Anthropic ключ", r"\bsk-(ant-|proj-)?[A-Za-z0-9_-]{32,}"),
        ("Пароль/секрет в коде", r#"(?i)\b(password|passwd|pwd|secret|api_?key|access_?token|auth_?token)\b\s*[:=]\s*["'][^"'\s]{8,}["']"#),
    ];
    p.iter().map(|(n, r)| (*n, Regex::new(r).unwrap())).collect()
}

const RISKY_NAMES: &[&str] = &[".env", "id_rsa", "id_ed25519", "id_dsa", "credentials.json", "secrets.json", ".npmrc", ".pypirc"];
const RISKY_EXT: &[&str] = &["pem", "key", "pfx", "p12", "keystore", "jks", "kdbx"];

/// Ищет секреты и опасные файлы. Возвращает (находки, большие файлы >50 МБ).
pub fn scan(d: &Path, files: &[String]) -> (Vec<Finding>, Vec<(String, u64)>) {
    let pats = patterns();
    let mut found = vec![];
    let mut big = vec![];
    for f in files {
        let p = d.join(f);
        let Ok(meta) = std::fs::metadata(&p) else { continue };
        if meta.is_dir() {
            continue;
        }
        if meta.len() > 50 * 1024 * 1024 {
            big.push((f.clone(), meta.len()));
        }
        let name = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ext = p.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        if (RISKY_NAMES.contains(&name.as_str()) || (name.starts_with(".env.") && !name.contains("example")))
            || RISKY_EXT.contains(&ext.as_str())
        {
            found.push(Finding { path: f.clone(), what: "Файл с секретами/ключами".into(), line: 0 });
            continue;
        }
        if meta.len() > 1024 * 1024 {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        if bytes.iter().take(4096).any(|&b| b == 0) {
            continue; // бинарный
        }
        let text = String::from_utf8_lossy(&bytes);
        'lines: for (i, l) in text.lines().enumerate() {
            if l.len() > 2000 {
                continue;
            }
            for (n, r) in &pats {
                if r.is_match(l) {
                    found.push(Finding { path: f.clone(), what: n.to_string(), line: i + 1 });
                    break 'lines;
                }
            }
        }
    }
    (found, big)
}

pub fn scan_screen(app: &mut App) {
    let Some(d) = need_project(app) else { return };
    ui::header("Проверка на секреты и большие файлы");
    let files: Vec<String> = if git::is_repo(&d) {
        git::run(&d, &["ls-files", "-co", "--exclude-standard"]).unwrap_or_default().lines().map(String::from).collect()
    } else {
        walk(&d)
    };
    let n = files.len();
    let (found, big) = ui::spin(&format!("Проверяю {} файлов", n), || scan(&d, &files));
    report_scan(&found, &big);
    if found.is_empty() && big.is_empty() {
        ui::success(&format!("Чисто! Проверено файлов: {}", n));
    } else if !found.is_empty() && ui::confirm("Добавить файлы с находками в .gitignore?", false) {
        let mut add: Vec<String> = found.iter().map(|f| format!("/{}", f.path)).collect();
        add.dedup();
        append_ignore(&d, &add);
        ui::success("Добавлено в .gitignore");
        gitignore::untrack_ignored(&d);
    }
    ui::pause();
}

fn walk(d: &Path) -> Vec<String> {
    let mut out = vec![];
    let mut stack = vec![d.to_path_buf()];
    while let Some(p) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&p) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if matches!(n.as_str(), ".git" | "node_modules" | "target" | ".venv" | "venv") {
                    continue;
                }
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(r) = path.strip_prefix(d) {
                    out.push(r.to_string_lossy().replace('\\', "/"));
                }
                if out.len() > 20000 {
                    return out;
                }
            }
        }
    }
    out
}

fn report_scan(found: &[Finding], big: &[(String, u64)]) {
    if !found.is_empty() {
        ui::blank();
        ui::warn(&format!("Найдены возможные секреты ({}):", found.len()));
        for f in found.iter().take(15) {
            let ln = if f.line > 0 { format!(":{}", f.line) } else { String::new() };
            ui::line(&format!("    {} {}{}  {}", c(ERR, "●"), c(WHITE, &f.path), c(DIM, &ln), c(WARN, &f.what)));
        }
    }
    if !big.is_empty() {
        ui::blank();
        ui::warn("Большие файлы (GitHub не примет файлы больше 100 МБ):");
        for (p, s) in big {
            let col = if *s > 100 * 1024 * 1024 { ERR } else { WARN };
            ui::line(&format!("    {} {}  {}", c(col, "●"), c(WHITE, p), c(col, &human(*s))));
        }
    }
    if !found.is_empty() || !big.is_empty() {
        ui::blank();
    }
}

fn append_ignore(d: &Path, lines: &[String]) {
    let p = d.join(".gitignore");
    let mut cur = std::fs::read_to_string(&p).unwrap_or_default();
    if !cur.is_empty() && !cur.ends_with('\n') {
        cur.push('\n');
    }
    for l in lines {
        if !cur.lines().any(|x| x.trim() == l) {
            cur.push_str(l);
            cur.push('\n');
        }
    }
    let _ = std::fs::write(p, cur);
}

// ───────────────────── коммит ─────────────────────

const TYPES: &[(&str, &str, &str)] = &[
    ("feat", "✦", "новая функциональность"),
    ("fix", "✚", "исправление ошибки"),
    ("docs", "▤", "документация"),
    ("style", "◐", "форматирование, без изменения логики"),
    ("refactor", "↻", "переработка кода"),
    ("perf", "⚡", "ускорение"),
    ("test", "✓", "тесты"),
    ("build", "⚙", "сборка и зависимости"),
    ("ci", "∞", "CI/CD"),
    ("chore", "·", "рутина"),
];

fn auto_message(sel: &[&Change]) -> String {
    let all_new = sel.iter().all(|c| c.kind() == "новый" || c.kind() == "добавлен");
    let all_del = sel.iter().all(|c| c.kind() == "удалён");
    let verb = if all_new { "Add" } else if all_del { "Remove" } else { "Update" };
    let names: Vec<String> = sel
        .iter()
        .take(3)
        .map(|c| Path::new(&c.path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or(c.path.clone()))
        .collect();
    if sel.len() <= 3 {
        format!("{} {}", verb, names.join(", "))
    } else {
        format!("{} {} and {} more files", verb, names.join(", "), sel.len() - 3)
    }
}

fn build_message(sel: &[&Change], first_commit: bool) -> Option<String> {
    ui::header("Коммит › сообщение");
    let mut items = vec![];
    if first_commit {
        items.push(Item::new("★", "Initial commit", "классика для первого коммита"));
    }
    items.push(Item::new("✎", "Написать своё сообщение", ""));
    items.push(Item::new("⚡", "Сгенерировать автоматически", &ui::truncate(&auto_message(sel), 50)));
    items.push(Item::sep("Conventional Commits"));
    for (t, i, d) in TYPES {
        items.push(Item::new(i, t, d));
    }
    let off = if first_commit { 1 } else { 0 };
    let choice = ui::menu(&items, 0)?;
    ui::blank();
    if first_commit && choice == 0 {
        return Some("Initial commit".into());
    }
    if choice == off {
        let subj = ui::input_req("Сообщение", "")?;
        let body = ui::input("Подробности (необязательно)", "", false)?;
        return Some(if body.is_empty() { subj } else { format!("{}\n\n{}", subj, body) });
    }
    if choice == off + 1 {
        return ui::input_req("Сообщение", &auto_message(sel));
    }
    let t = TYPES[choice - off - 3].0;
    let scope = ui::input("Область (scope, необязательно, напр. api)", "", false)?;
    let subj = ui::input_req("Кратко, что сделано", "")?;
    let body = ui::input("Подробности (необязательно)", "", false)?;
    let breaking = ui::confirm("Ломает обратную совместимость (BREAKING CHANGE)?", false);
    let head = format!(
        "{}{}{}: {}",
        t,
        if scope.is_empty() { String::new() } else { format!("({})", scope) },
        if breaking { "!" } else { "" },
        subj
    );
    let mut msg = head;
    if !body.is_empty() {
        msg.push_str(&format!("\n\n{}", body));
    }
    Some(msg)
}

/// Полный цикл коммита. Возвращает true, если коммит создан.
pub fn commit(app: &mut App, d: &Path) -> bool {
    ui::header("Коммит");
    if !git::is_repo(d) && !init_repo(app, d) {
        return false;
    }
    if !ensure_identity(app) {
        return false;
    }
    let changes = ui::spin("Ищу изменения", || git::status(d));
    if changes.is_empty() {
        ui::success("Нечего коммитить — всё уже сохранено");
        ui::pause();
        return false;
    }
    let mut checks: Vec<Check> = changes
        .iter()
        .map(|ch| Check { label: ch.path.clone(), hint: ch.kind().to_string(), on: true, color: change_color(ch) })
        .collect();
    if !ui::checklist(&format!("Что включить в коммит? (изменений: {})", changes.len()), &mut checks) {
        return false;
    }
    let mut sel: Vec<&Change> = changes.iter().enumerate().filter(|(i, _)| checks[*i].on).map(|(_, c)| c).collect();
    if sel.is_empty() {
        ui::warn("Ничего не выбрано");
        ui::pause();
        return false;
    }

    // безопасность
    if app.cfg.scan_secrets {
        let files: Vec<String> = sel.iter().filter(|c| c.kind() != "удалён").map(|c| c.path.clone()).collect();
        let (found, big) = ui::spin("Проверяю на секреты и большие файлы", || scan(d, &files));
        if !found.is_empty() || !big.is_empty() {
            ui::header("Коммит › безопасность");
            report_scan(&found, &big);
            let huge: Vec<String> = big.iter().filter(|(_, s)| *s > 100 * 1024 * 1024).map(|(p, _)| p.clone()).collect();
            if !huge.is_empty() {
                ui::err("Файлы больше 100 МБ GitHub отклонит — исключаю их из коммита.");
                sel.retain(|c| !huge.contains(&c.path));
            }
            if !found.is_empty() {
                let opts = [
                    "Исключить эти файлы и добавить в .gitignore (рекомендую)",
                    "Только исключить из этого коммита",
                    "Это не секреты — продолжить",
                    "Отменить коммит",
                ];
                match ui::choose("Что сделать?", &opts) {
                    Some(0) => {
                        let bad: Vec<String> = found.iter().map(|f| f.path.clone()).collect();
                        append_ignore(d, &bad.iter().map(|p| format!("/{}", p)).collect::<Vec<_>>());
                        sel.retain(|c| !bad.contains(&c.path));
                        ui::success("Исключено и добавлено в .gitignore");
                        // сам .gitignore тоже коммитим
                    }
                    Some(1) => {
                        let bad: Vec<String> = found.iter().map(|f| f.path.clone()).collect();
                        sel.retain(|c| !bad.contains(&c.path));
                    }
                    Some(2) => {}
                    _ => return false,
                }
            }
            if sel.is_empty() {
                ui::warn("После исключений коммитить нечего");
                ui::pause();
                return false;
            }
        }
    }

    let first = !git::has_commits(d);
    let Some(msg) = build_message(&sel, first) else { return false };

    let mut paths: Vec<String> = sel.iter().map(|c| c.path.clone()).collect();
    if d.join(".gitignore").exists() && !paths.iter().any(|p| p == ".gitignore") {
        if changes.iter().any(|c| c.path == ".gitignore") || git::status(d).iter().any(|c| c.path == ".gitignore") {
            paths.push(".gitignore".into());
        }
    }
    ui::header("Коммит › сохранение");
    ui::kv("Файлов", &paths.len().to_string());
    ui::kv("Сообщение", msg.lines().next().unwrap_or(""));
    ui::blank();
    let res = ui::spin("Создаю коммит", || -> Result<String, String> {
        if !first {
            let _ = git::run(d, &["reset", "-q"]);
        }
        for chunk in paths.chunks(80) {
            let mut a = vec!["add", "-A", "--"];
            a.extend(chunk.iter().map(|s| s.as_str()));
            git::run(d, &a)?;
        }
        git::run(d, &["commit", "-q", "-m", &msg])?;
        git::run(d, &["log", "-1", "--format=%h"])
    });
    match res {
        Ok(h) => {
            ui::success(&format!("Коммит {} создан", h));
            true
        }
        Err(e) => {
            ui::err(&e);
            ui::pause();
            false
        }
    }
}

pub fn commit_screen(app: &mut App) {
    let Some(d) = need_project(app) else { return };
    if commit(app, &d) {
        ui::blank();
        if git::remote_url(&d).is_some() && ui::confirm("Отправить на GitHub (push)?", true) {
            push(app, &d);
        }
        ui::pause();
    }
}

// ───────────────────── публикация ─────────────────────

pub fn auth_for(app: &App, d: &Path) -> Option<(String, String)> {
    let url = git::remote_url(d)?;
    if !url.contains("github.com") {
        return None;
    }
    Some((app.login.clone()?, app.token.clone()?))
}

pub fn push(app: &App, d: &Path) -> bool {
    let br = git::branch(d);
    let auth = auth_for(app, d);
    let res = ui::spin(&format!("Отправляю ветку {} на GitHub", br), || {
        git::run_net(d, &["push", "-u", "origin", &br], auth.as_ref().map(|(l, t)| (l.as_str(), t.as_str())))
    });
    match res {
        Ok(_) => {
            ui::success("Изменения отправлены");
            true
        }
        Err(e) => {
            ui::err("Push не удался:");
            for l in e.lines().take(8) {
                ui::dim(&format!("   {}", l));
            }
            if e.contains("rejected") || e.contains("fetch first") {
                ui::info("На GitHub есть изменения, которых нет у тебя — сначала сделай «Получить (pull)».");
            }
            false
        }
    }
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "my-project".into()
    } else {
        s
    }
}

/// Мастер «папка → .gitignore → коммит → репозиторий на GitHub → push».
pub fn publish_wizard(app: &mut App, ask_folder: bool) {
    if ask_folder || app.project.is_none() {
        if !select_folder(app) {
            return;
        }
    }
    let d = app.project.clone().unwrap();
    if !auth::require(app) {
        return;
    }
    let steps = ["Репозиторий", ".gitignore", "Коммит", "GitHub", "Отправка"];
    let show_steps = |cur: usize| {
        let mut s = String::from("  ");
        for (i, st) in steps.iter().enumerate() {
            let (col, mark) = if i < cur { (OK, "✔") } else if i == cur { (CYAN, "◉") } else { (FAINT, "○") };
            s.push_str(&format!("{}{} {}", fg(col), mark, st));
            if i + 1 < steps.len() {
                s.push_str(&c(FAINT, "  ──  "));
            }
        }
        ui::line(&s);
        ui::blank();
    };

    // 1. репозиторий
    ui::header("Публикация на GitHub");
    show_steps(0);
    if !git::is_repo(&d) && !init_repo(app, &d) {
        return;
    }
    // 2. .gitignore
    let fresh = !git::has_commits(&d);
    if fresh || !d.join(".gitignore").exists() {
        if !gitignore::wizard(&d) {
            return;
        }
    }
    // 3. коммит
    ui::header("Публикация на GitHub");
    show_steps(2);
    if !git::status(&d).is_empty() {
        commit(app, &d);
    }
    if !git::has_commits(&d) {
        ui::err("Нужен хотя бы один коммит, чтобы опубликовать проект.");
        ui::pause();
        return;
    }
    // 4. репозиторий на GitHub
    ui::header("Публикация на GitHub");
    show_steps(3);
    let token = app.token.clone().unwrap();
    let login = app.login.clone().unwrap();
    let mut html = String::new();
    if let Some(url) = git::remote_url(&d) {
        ui::info(&format!("Remote уже подключён: {}", c(CYAN, &url)));
        if let Some((o, r)) = git::github_slug(&url) {
            html = format!("https://github.com/{}/{}", o, r);
        }
    } else {
        let folder = d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let Some(name) = ui::input_req("Название репозитория", &sanitize(&folder)) else { return };
        let name = sanitize(&name);
        let Some(desc) = ui::input("Описание (необязательно)", "", false) else { return };
        ui::blank();
        let vis = if app.cfg.default_private { 0 } else { 1 };
        let items = [
            Item::new("⚿", "Приватный", "видишь только ты и кого пригласишь"),
            Item::new("◎", "Публичный", "виден всем"),
        ];
        ui::line(&format!("  {} {}", c(CYAN, "?"), c(WHITE, "Видимость")));
        let Some(v) = ui::menu(&items, vis) else { return };
        ui::blank();
        let private = v == 0;
        let res = ui::spin("Создаю репозиторий на GitHub", || github::create_repo(&token, &name, &desc, private));
        let repo = match res {
            Ok(r) => {
                ui::success(&format!("Репозиторий {} создан", r.full_name));
                r
            }
            Err(e) if e.contains("422") => {
                ui::warn(&format!("У тебя уже есть репозиторий {}", name));
                if !ui::confirm("Подключить проект к нему?", true) {
                    return;
                }
                match ui::spin("Загружаю репозиторий", || github::get_repo(&token, &login, &name)) {
                    Ok(r) => r,
                    Err(e) => {
                        ui::err(&e);
                        ui::pause();
                        return;
                    }
                }
            }
            Err(e) => {
                ui::err(&e);
                ui::pause();
                return;
            }
        };
        if let Err(e) = git::run(&d, &["remote", "add", "origin", &repo.clone_url]) {
            ui::err(&e);
            ui::pause();
            return;
        }
        html = repo.html_url;
    }
    // 5. push
    ui::header("Публикация на GitHub");
    show_steps(4);
    if push(app, &d) {
        ui::blank();
        celebrate(&html);
        if app.cfg.auto_open_browser && !html.is_empty() {
            let _ = open::that(&html);
        }
    }
    ui::pause();
}

fn celebrate(url: &str) {
    if ui::anim_on() {
        let w = 46;
        for i in 0..=w {
            let mut s = String::from("  ");
            for j in 0..i {
                s.push_str(&fg(ui::grad(j as f32 / w as f32)));
                s.push(if (i + j) % 7 == 0 { '✦' } else { '━' });
            }
            ui::out(&format!("\r{}{}\x1b[K", ui::bg(ui::BG), s));
            ui::frame(10);
        }
        ui::out("\r\n");
    }
    ui::boxed(
        &[
            format!("{}  {}", c(OK, "★"), c(WHITE, "Проект опубликован!")),
            String::new(),
            c(CYAN, url),
        ],
        OK,
    );
}

// ───────────────────── синхронизация ─────────────────────

pub fn sync_screen(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    loop {
        ui::header("Синхронизация");
        if git::remote_url(&d).is_none() {
            ui::warn("Проект не подключён к GitHub.");
            if ui::confirm("Опубликовать сейчас?", true) {
                publish_wizard(app, false);
            }
            return;
        }
        let auth = auth_for(app, &d);
        let a2 = auth.clone();
        let _ = ui::spin("Проверяю GitHub", || git::run_net(&d, &["fetch", "--quiet"], a2.as_ref().map(|(l, t)| (l.as_str(), t.as_str()))));
        let (ahead, behind) = git::ahead_behind(&d).unwrap_or((0, 0));
        ui::kv("Ветка", &format!("⎇ {}", git::branch(&d)));
        ui::kv("К отправке", &c(if ahead > 0 { WARN } else { DIM }, &format!("↑ {}", ahead)));
        ui::kv("К получению", &c(if behind > 0 { WARN } else { DIM }, &format!("↓ {}", behind)));
        let dirty = !git::status(&d).is_empty();
        if dirty {
            ui::kv("Рабочая папка", &c(WARN, "есть незакоммиченные изменения"));
        }
        ui::blank();
        let items = vec![
            Item::new("⇅", "Синхронизировать", "получить, затем отправить"),
            Item::new("↑", "Отправить (push)", ""),
            Item::new("↓", "Получить (pull --rebase)", "аккуратно, без лишних merge-коммитов"),
            Item::new("↓", "Получить (pull, merge)", ""),
            Item::new("✍", "Закоммитить и отправить", ""),
        ];
        let au = auth.as_ref().map(|(l, t)| (l.as_str(), t.as_str()));
        match ui::menu(&items, 0) {
            Some(0) => {
                if pull(&d, true, au) {
                    push(app, &d);
                }
                ui::pause();
            }
            Some(1) => {
                push(app, &d);
                ui::pause();
            }
            Some(2) => {
                pull(&d, true, au);
                ui::pause();
            }
            Some(3) => {
                pull(&d, false, au);
                ui::pause();
            }
            Some(4) => {
                if commit(app, &d) {
                    push(app, &d);
                }
                ui::pause();
            }
            _ => return,
        }
    }
}

fn pull(d: &Path, rebase: bool, auth: Option<(&str, &str)>) -> bool {
    let mut args = vec!["pull"];
    if rebase {
        args.extend(["--rebase", "--autostash"]);
    } else {
        args.push("--no-rebase");
    }
    match ui::spin("Получаю изменения", || git::run_net(d, &args, auth)) {
        Ok(o) => {
            ui::success(if o.contains("up to date") || o.contains("Already") { "Уже актуально" } else { "Изменения получены" });
            true
        }
        Err(e) => {
            ui::err("Не удалось получить изменения:");
            for l in e.lines().take(8) {
                ui::dim(&format!("   {}", l));
            }
            if e.contains("CONFLICT") || e.contains("conflict") {
                ui::warn("Конфликт! Исправь файлы с пометками <<<<<<< и сделай коммит,");
                ui::warn("или отмени: «Исправить ошибки» → «Отменить незавершённый rebase/merge».");
            }
            false
        }
    }
}

