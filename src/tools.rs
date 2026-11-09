//! Дополнительные инструменты: ветки, история, stash, откат, релизы, README/LICENSE,
//! статистика, клонирование, браузер, настройки, установка Git.

use crate::auth;
use crate::git;
use crate::github;
use crate::project::{self, auth_for, need_project, need_repo};
use crate::ui::{self, c, fg, Check, Item, CYAN, DIM, ERR, FAINT, ICE, OK, PRIMARY, WARN, WHITE};
use crate::App;
use std::path::{Path, PathBuf};

fn report(r: Result<String, String>, ok_msg: &str) -> bool {
    match r {
        Ok(_) => {
            ui::success(ok_msg);
            true
        }
        Err(e) => {
            ui::err(e.lines().next().unwrap_or("ошибка"));
            for l in e.lines().skip(1).take(6) {
                ui::dim(&format!("   {}", l));
            }
            false
        }
    }
}

// ───────────────────── ветки ─────────────────────

pub fn branches(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    loop {
        ui::header("Ветки");
        let cur = git::branch(&d);
        let list: Vec<(String, String)> = git::run(&d, &["branch", "--format=%(refname:short)\x1f%(committerdate:relative)"])
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\x1f');
                Some((p.next()?.to_string(), p.next().unwrap_or("").to_string()))
            })
            .collect();
        ui::kv("Текущая", &c(CYAN, &format!("⎇ {}", if cur.is_empty() { "(нет коммитов)" } else { &cur })));
        ui::blank();
        let mut items = vec![
            Item::new("＋", "Создать ветку", "и сразу переключиться"),
            Item::new("⇄", "Слить ветку в текущую (merge)", ""),
            Item::new("✎", "Переименовать текущую", ""),
            Item::new("✖", "Удалить ветку", ""),
            Item::sep("Переключиться"),
        ];
        for (b, when) in &list {
            let icon = if b == &cur { "●" } else { "○" };
            items.push(Item::new(icon, b, when));
        }
        let Some(ch) = ui::menu(&items, 0) else { return };
        ui::blank();
        match ch {
            0 => {
                if let Some(n) = ui::input_req("Имя новой ветки (напр. feature/login)", "") {
                    let n = n.replace(' ', "-");
                    report(git::run(&d, &["switch", "-c", &n]), &format!("Создана и выбрана ветка {}", n));
                    ui::pause();
                }
            }
            1 => {
                let others: Vec<&str> = list.iter().map(|(b, _)| b.as_str()).filter(|b| *b != cur).collect();
                if others.is_empty() {
                    ui::info("Других веток нет");
                } else if let Some(i) = ui::choose(&format!("Какую ветку влить в {}?", cur), &others) {
                    let r = ui::spin("Сливаю", || git::run(&d, &["merge", "--no-edit", others[i]]));
                    if !report(r, "Ветки слиты") {
                        ui::warn("Если возник конфликт — исправь файлы и закоммить, либо отмени через «Исправить ошибки».");
                    }
                }
                ui::pause();
            }
            2 => {
                if let Some(n) = ui::input_req("Новое имя", &cur) {
                    report(git::run(&d, &["branch", "-m", &n]), "Ветка переименована");
                    ui::pause();
                }
            }
            3 => {
                let others: Vec<&str> = list.iter().map(|(b, _)| b.as_str()).filter(|b| *b != cur).collect();
                if others.is_empty() {
                    ui::info("Удалять нечего — текущую ветку удалить нельзя");
                } else if let Some(i) = ui::choose("Какую ветку удалить?", &others) {
                    if ui::confirm(&format!("Удалить {}?", others[i]), false) {
                        let r = git::run(&d, &["branch", "-d", others[i]]);
                        if let Err(e) = &r {
                            if e.contains("not fully merged") {
                                ui::warn("В ветке есть не слитые коммиты.");
                                if ui::confirm("Удалить принудительно (коммиты потеряются)?", false) {
                                    report(git::run(&d, &["branch", "-D", others[i]]), "Ветка удалена");
                                }
                            } else {
                                report(r, "");
                            }
                        } else {
                            ui::success("Ветка удалена");
                        }
                    }
                }
                ui::pause();
            }
            n => {
                let b = &list[n - 5].0;
                if b != &cur {
                    let r = ui::spin("Переключаюсь", || git::run(&d, &["switch", b]));
                    if !report(r, &format!("Теперь на ветке {}", b)) {
                        ui::info("Подсказка: незакоммиченные изменения можно спрятать через «Stash».");
                    }
                    ui::pause();
                }
            }
        }
    }
}

// ───────────────────── история ─────────────────────

pub fn history(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    if !git::has_commits(&d) {
        ui::header("История");
        ui::info("Коммитов пока нет");
        ui::pause();
        return;
    }
    let raw = ui::spin("Читаю историю", || {
        git::run(&d, &["log", "--all", "--graph", "-n", "500", "--format=\x1f%h\x1f%d\x1f%s\x1f%an\x1f%ar"]).unwrap_or_default()
    });
    let lines: Vec<String> = raw
        .lines()
        .map(|l| {
            let parts: Vec<&str> = l.split('\x1f').collect();
            if parts.len() < 6 {
                return c(DIM, l);
            }
            let graph: String = parts[0]
                .chars()
                .map(|ch| match ch {
                    '*' => format!("{}●", fg(CYAN)),
                    '|' => format!("{}│", fg(ui::DEEP)),
                    '/' => format!("{}╱", fg(ui::DEEP)),
                    '\\' => format!("{}╲", fg(ui::DEEP)),
                    _ => ch.to_string(),
                })
                .collect();
            format!(
                "{} {}{}{}{} {}{}  {}{} · {}",
                graph,
                fg(WARN),
                parts[1],
                fg(OK),
                parts[2],
                fg(WHITE),
                parts[3],
                fg(DIM),
                parts[4],
                parts[5]
            ) + &fg(PRIMARY)
        })
        .collect();
    ui::pager("История коммитов", &lines);
}

// ───────────────────── stash ─────────────────────

pub fn stash(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    loop {
        ui::header("Stash — временно отложить изменения");
        let list: Vec<String> = git::run(&d, &["stash", "list", "--format=%gd\x1f%s\x1f%cr"])
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect();
        ui::dim("Удобно, когда нужно срочно переключиться на другую ветку, не коммитя недоделанное.");
        ui::blank();
        let mut items = vec![Item::new("⤓", "Отложить текущие изменения", "включая новые файлы")];
        if !list.is_empty() {
            items.push(Item::sep(&format!("Отложено ({})", list.len())));
        }
        for s in &list {
            let p: Vec<&str> = s.split('\x1f').collect();
            items.push(Item::new("◫", p.get(1).copied().unwrap_or(""), &format!("{} · {}", p.first().unwrap_or(&""), p.get(2).unwrap_or(&""))));
        }
        let Some(ch) = ui::menu(&items, 0) else { return };
        ui::blank();
        if ch == 0 {
            let msg = ui::input("Подпись (необязательно)", "", false).unwrap_or_default();
            let mut a = vec!["stash", "push", "-u"];
            if !msg.is_empty() {
                a.extend(["-m", msg.as_str()]);
            }
            report(git::run(&d, &a), "Изменения отложены, рабочая папка чистая");
            ui::pause();
            continue;
        }
        let entry = list[ch - 2].split('\x1f').next().unwrap_or("").to_string();
        let acts = ["Вернуть и удалить из списка (pop)", "Вернуть, оставив копию (apply)", "Посмотреть изменения", "Удалить"];
        match ui::choose(&format!("Что сделать с {}?", entry), &acts) {
            Some(0) => {
                report(git::run(&d, &["stash", "pop", &entry]), "Изменения возвращены");
                ui::pause();
            }
            Some(1) => {
                report(git::run(&d, &["stash", "apply", &entry]), "Изменения применены");
                ui::pause();
            }
            Some(2) => {
                let t = git::run(&d, &["stash", "show", "-p", "--include-untracked", &entry]).unwrap_or_default();
                ui::pager(&format!("Stash {}", entry), &project::colorize_diff(&t));
            }
            Some(3) => {
                if ui::confirm("Удалить безвозвратно?", false) {
                    report(git::run(&d, &["stash", "drop", &entry]), "Удалено");
                    ui::pause();
                }
            }
            _ => {}
        }
    }
}

// ───────────────────── откат и исправление ошибок ─────────────────────

pub fn undo(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    loop {
        ui::header("Исправить ошибки");
        let in_progress = if d.join(".git/rebase-merge").exists() || d.join(".git/rebase-apply").exists() {
            Some("rebase")
        } else if d.join(".git/MERGE_HEAD").exists() {
            Some("merge")
        } else {
            None
        };
        let mut items = vec![
            Item::new("↶", "Отменить последний коммит", "изменения останутся в файлах"),
            Item::new("✎", "Исправить сообщение последнего коммита", "amend"),
            Item::new("＋", "Добавить забытые файлы в последний коммит", "amend"),
            Item::new("⟲", "Откатить изменения в файлах", "вернуть как было в последнем коммите"),
            Item::new("⎌", "Отменить старый коммит (revert)", "безопасно, через новый коммит"),
            Item::new("↺", "Журнал действий (reflog)", "вернуться к любому прошлому состоянию"),
        ];
        if let Some(p) = in_progress {
            items.insert(0, Item::new("⚠", &format!("Отменить незавершённый {}", p), "вернуть всё как было до него"));
        }
        let off = if in_progress.is_some() { 1 } else { 0 };
        let Some(ch) = ui::menu(&items, 0) else { return };
        ui::blank();
        if in_progress.is_some() && ch == 0 {
            let p = in_progress.unwrap();
            report(git::run(&d, &[p, "--abort"]), "Отменено");
            ui::pause();
            continue;
        }
        let pushed = git::ahead_behind(&d).map(|(a, _)| a == 0).unwrap_or(false);
        match ch - off {
            0 => {
                let last = git::last_commit(&d).unwrap_or_default();
                ui::kv("Коммит", &last);
                if pushed {
                    ui::warn("Этот коммит уже на GitHub — после отмены понадобится push --force.");
                }
                if ui::confirm("Отменить его (файлы не пострадают)?", !pushed) {
                    let r = if git::run(&d, &["rev-parse", "HEAD~1"]).is_ok() {
                        git::run(&d, &["reset", "--soft", "HEAD~1"])
                    } else {
                        git::run(&d, &["update-ref", "-d", "HEAD"])
                    };
                    report(r, "Коммит отменён, изменения сохранены в файлах");
                }
                ui::pause();
            }
            1 => {
                let cur = git::run(&d, &["log", "-1", "--format=%B"]).unwrap_or_default();
                if pushed {
                    ui::warn("Коммит уже на GitHub — после изменения понадобится push --force.");
                }
                if let Some(m) = ui::input_req("Новое сообщение", cur.lines().next().unwrap_or("")) {
                    report(git::run(&d, &["commit", "--amend", "-m", &m]), "Сообщение исправлено");
                }
                ui::pause();
            }
            2 => {
                let ch = git::status(&d);
                let mut checks: Vec<Check> =
                    ch.iter().map(|x| Check { label: x.path.clone(), hint: x.kind().into(), on: false, color: ICE }).collect();
                if checks.is_empty() {
                    ui::info("Нет изменённых файлов");
                } else if ui::checklist("Какие файлы добавить в последний коммит?", &mut checks) {
                    let files: Vec<&str> = ch.iter().enumerate().filter(|(i, _)| checks[*i].on).map(|(_, x)| x.path.as_str()).collect();
                    if !files.is_empty() {
                        let mut a = vec!["add", "-A", "--"];
                        a.extend(files.iter());
                        let r = git::run(&d, &a).and_then(|_| git::run(&d, &["commit", "--amend", "--no-edit"]));
                        report(r, "Файлы добавлены в последний коммит");
                    }
                }
                ui::pause();
            }
            3 => {
                let ch = git::status(&d);
                let mut checks: Vec<Check> =
                    ch.iter().map(|x| Check { label: x.path.clone(), hint: x.kind().into(), on: false, color: WARN }).collect();
                if checks.is_empty() {
                    ui::info("Нет изменений");
                } else if ui::checklist("Какие изменения выбросить? (ОСТОРОЖНО: необратимо)", &mut checks) {
                    let sel: Vec<&git::Change> = ch.iter().enumerate().filter(|(i, _)| checks[*i].on).map(|(_, x)| x).collect();
                    if !sel.is_empty() && ui::confirm(&format!("Выбросить изменения в {} файлах?", sel.len()), false) {
                        for x in sel {
                            let r = if x.kind() == "новый" {
                                std::fs::remove_file(d.join(&x.path)).map(|_| String::new()).map_err(|e| e.to_string())
                            } else {
                                git::run(&d, &["restore", "--staged", "--worktree", "--source=HEAD", "--", &x.path])
                            };
                            if let Err(e) = r {
                                ui::err(&format!("{}: {}", x.path, e));
                            }
                        }
                        ui::success("Готово");
                    }
                }
                ui::pause();
            }
            4 => {
                let log = git::run(&d, &["log", "-n", "30", "--format=%h\x1f%s\x1f%ar"]).unwrap_or_default();
                let rows: Vec<Vec<String>> = log.lines().map(|l| l.split('\x1f').map(String::from).collect()).collect();
                let items: Vec<Item> = rows.iter().map(|r| Item::new(&r[0], &r[1], &r[2])).collect();
                ui::line(&format!("  {} {}", c(CYAN, "?"), c(WHITE, "Какой коммит отменить?")));
                if let Some(i) = ui::menu(&items, 0) {
                    ui::blank();
                    report(git::run(&d, &["revert", "--no-edit", &rows[i][0]]), "Создан коммит, отменяющий изменения");
                    ui::pause();
                }
            }
            5 => {
                let log = git::run(&d, &["reflog", "-n", "40", "--format=%h\x1f%gs\x1f%cr"]).unwrap_or_default();
                let rows: Vec<Vec<String>> = log.lines().map(|l| l.split('\x1f').map(String::from).collect()).collect();
                let items: Vec<Item> = rows.iter().map(|r| Item::new(&r[0], r.get(1).map(|s| s.as_str()).unwrap_or(""), r.get(2).map(|s| s.as_str()).unwrap_or(""))).collect();
                ui::line(&format!("  {} {}", c(CYAN, "?"), c(WHITE, "К какому состоянию вернуться?")));
                if let Some(i) = ui::menu(&items, 0) {
                    ui::blank();
                    ui::warn("Незакоммиченные изменения будут отложены в stash, чтобы не потерялись.");
                    if ui::confirm(&format!("Вернуться к {}?", rows[i][0]), false) {
                        let _ = git::run(&d, &["stash", "push", "-u", "-m", "GitHelper: перед возвратом по reflog"]);
                        report(git::run(&d, &["reset", "--hard", &rows[i][0]]), "Состояние восстановлено");
                    }
                    ui::pause();
                }
            }
            _ => {}
        }
    }
}

// ───────────────────── релизы ─────────────────────

fn bump(tag: &str, part: usize) -> String {
    let pre = if tag.starts_with('v') { "v" } else { "" };
    let nums: Vec<u64> = tag.trim_start_matches('v').split('.').map(|x| x.parse().unwrap_or(0)).collect();
    let (mut a, mut b, mut c2) = (*nums.first().unwrap_or(&0), *nums.get(1).unwrap_or(&0), *nums.get(2).unwrap_or(&0));
    match part {
        0 => c2 += 1,
        1 => {
            b += 1;
            c2 = 0
        }
        _ => {
            a += 1;
            b = 0;
            c2 = 0
        }
    }
    format!("{}{}.{}.{}", pre, a, b, c2)
}

pub fn release(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    ui::header("Релиз / тег версии");
    if !git::has_commits(&d) {
        ui::warn("Сначала нужен хотя бы один коммит");
        ui::pause();
        return;
    }
    let last = git::run(&d, &["describe", "--tags", "--abbrev=0"]).ok();
    ui::kv("Последняя версия", last.as_deref().unwrap_or("тегов ещё нет"));
    ui::blank();
    let base = last.clone().unwrap_or_else(|| "v0.0.0".into());
    let opts_s = [
        format!("{}  — исправления (patch)", bump(&base, 0)),
        format!("{}  — новые возможности (minor)", bump(&base, 1)),
        format!("{}  — большие изменения (major)", bump(&base, 2)),
        "Своя версия…".to_string(),
    ];
    let opts: Vec<&str> = opts_s.iter().map(|s| s.as_str()).collect();
    let Some(ch) = ui::choose("Какую версию выпустить?", &opts) else { return };
    ui::blank();
    let tag = if ch < 3 {
        bump(&base, ch)
    } else {
        match ui::input_req("Версия", "v1.0.0") {
            Some(t) => t,
            None => return,
        }
    };
    // заметки из коммитов
    let range = last.map(|l| format!("{}..HEAD", l)).unwrap_or_else(|| "HEAD".into());
    let notes = git::run(&d, &["log", &range, "--format=- %s (%h)", "--no-merges"]).unwrap_or_default();
    ui::info(&format!("Коммитов в релизе: {}", notes.lines().count()));
    let title = ui::input("Название релиза", &tag, false).unwrap_or(tag.clone());
    let r = git::run(&d, &["tag", "-a", &tag, "-m", &title]);
    if !report(r, &format!("Тег {} создан", tag)) {
        ui::pause();
        return;
    }
    let Some(url) = git::remote_url(&d) else {
        ui::info("Remote не подключён — тег создан только локально.");
        ui::pause();
        return;
    };
    if !ui::confirm("Отправить тег и создать релиз на GitHub?", true) {
        ui::pause();
        return;
    }
    let auth = auth_for(app, &d);
    let r = ui::spin("Отправляю тег", || {
        git::run_net(&d, &["push", "origin", &tag], auth.as_ref().map(|(l, t)| (l.as_str(), t.as_str())))
    });
    if !report(r, "Тег на GitHub") {
        ui::pause();
        return;
    }
    if let (Some((o, n)), Some(tok)) = (git::github_slug(&url), app.token.clone()) {
        let pre = ui::confirm("Это предварительная версия (pre-release)?", false);
        let body = format!("## Изменения\n\n{}", notes);
        match ui::spin("Создаю релиз", || github::create_release(&tok, &o, &n, &tag, &title, &body, pre)) {
            Ok(u) => {
                ui::success("Релиз опубликован");
                ui::kv("Ссылка", &u);
                if app.cfg.auto_open_browser {
                    let _ = open::that(u);
                }
            }
            Err(e) => ui::err(&e),
        }
    }
    ui::pause();
}

// ───────────────────── README и LICENSE ─────────────────────

pub fn docs(app: &mut App) {
    let Some(d) = need_project(app) else { return };
    loop {
        ui::header("README и LICENSE");
        let items = vec![
            Item::new("▤", "Создать README.md", if d.join("README.md").exists() { "уже есть — перезапишу" } else { "по шаблону" }),
            Item::new("§", "Добавить LICENSE", if d.join("LICENSE").exists() { "уже есть" } else { "MIT, Apache, GPL…" }),
        ];
        match ui::menu(&items, 0) {
            Some(0) => readme(app, &d),
            Some(1) => license(app, &d),
            _ => return,
        }
    }
}

fn readme(app: &App, d: &Path) {
    ui::header("README.md");
    let folder = d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let Some(name) = ui::input_req("Название проекта", &folder) else { return };
    let Some(desc) = ui::input("Короткое описание", "", false) else { return };
    let stacks = crate::gitignore::detect(d);
    let st: Vec<&str> = stacks.iter().map(|&i| crate::gitignore::STACKS[i].name).collect();
    let (install, run) = if st.iter().any(|s| s.starts_with("Rust")) {
        ("cargo build --release", "cargo run")
    } else if st.iter().any(|s| s.starts_with("Node")) {
        ("npm install", "npm start")
    } else if st.iter().any(|s| s.starts_with("Python")) {
        ("pip install -r requirements.txt", "python main.py")
    } else if st.iter().any(|s| s.starts_with("C#")) {
        ("dotnet build", "dotnet run")
    } else if st.iter().any(|s| s.starts_with("Go")) {
        ("go build", "go run .")
    } else {
        ("# команды установки", "# команда запуска")
    };
    let slug = git::remote_url(d).and_then(|u| git::github_slug(&u));
    let clone = match &slug {
        Some((o, r)) => format!("git clone https://github.com/{}/{}.git\ncd {}", o, r, r),
        None => format!("git clone <url>\ncd {}", name),
    };
    let author = app.login.clone().map(|l| format!("[@{}](https://github.com/{})", l, l)).unwrap_or_default();
    let lic = if d.join("LICENSE").exists() { "Распространяется под лицензией из файла [LICENSE](LICENSE)." } else { "Лицензия пока не выбрана." };
    let body = format!(
        "# {name}\n\n{desc}\n\n## Возможности\n\n- …\n- …\n\n## Установка\n\n```bash\n{clone}\n{install}\n```\n\n## Запуск\n\n```bash\n{run}\n```\n\n## Лицензия\n\n{lic}\n\n---\n\nАвтор: {author}\n"
    );
    if d.join("README.md").exists() && !ui::confirm("README.md уже существует. Перезаписать?", false) {
        return;
    }
    match std::fs::write(d.join("README.md"), body) {
        Ok(_) => ui::success("README.md создан — допиши раздел «Возможности»"),
        Err(e) => ui::err(&e.to_string()),
    }
    ui::pause();
}

fn license(app: &App, d: &Path) {
    ui::header("LICENSE");
    let opts = [
        ("mit", "MIT", "максимально свободная, самая популярная"),
        ("apache-2.0", "Apache 2.0", "свободная + защита патентов"),
        ("gpl-3.0", "GPL v3", "производные тоже должны быть открыты"),
        ("bsd-3-clause", "BSD 3-Clause", "похожа на MIT"),
        ("unlicense", "Unlicense", "общественное достояние"),
    ];
    let items: Vec<Item> = opts.iter().map(|(_, n, h)| Item::new("§", n, h)).collect();
    let Some(i) = ui::menu(&items, 0) else { return };
    ui::blank();
    let year = git::run(d, &["log", "-1", "--format=%cd", "--date=format:%Y"]).unwrap_or_else(|_| "2026".into());
    let holder = app.cfg.name.clone().or_else(|| git::config_get("user.name")).unwrap_or_else(|| "Author".into());
    let key = opts[i].0;
    let text = match ui::spin("Загружаю текст лицензии", || github::license_get(key)) {
        Ok(t) => t,
        Err(_) if key == "mit" => MIT.to_string(),
        Err(e) => {
            ui::err(&e);
            ui::pause();
            return;
        }
    };
    let text = text
        .replace("[year]", &year)
        .replace("[yyyy]", &year)
        .replace("[fullname]", &holder)
        .replace("[name of copyright owner]", &holder);
    if d.join("LICENSE").exists() && !ui::confirm("LICENSE уже есть. Заменить?", false) {
        return;
    }
    match std::fs::write(d.join("LICENSE"), text) {
        Ok(_) => ui::success(&format!("LICENSE ({}) добавлен", opts[i].1)),
        Err(e) => ui::err(&e.to_string()),
    }
    ui::pause();
}

const MIT: &str = "MIT License\n\nCopyright (c) [year] [fullname]\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the \"Software\"), to deal\nin the Software without restriction, including without limitation the rights\nto use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is\nfurnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\nFITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\nAUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\nLIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\nOUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\nSOFTWARE.\n";

// ───────────────────── статистика ─────────────────────

pub fn stats(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    ui::header("Статистика репозитория");
    if !git::has_commits(&d) {
        ui::info("Коммитов пока нет");
        ui::pause();
        return;
    }
    let (total, authors, first, files, times) = ui::spin("Анализирую репозиторий", || {
        let total = git::run(&d, &["rev-list", "--count", "HEAD"]).unwrap_or_default();
        let authors = git::run(&d, &["shortlog", "-sn", "--no-merges", "HEAD"]).unwrap_or_default();
        let first = git::run(&d, &["log", "--reverse", "--format=%ad", "--date=short"]).unwrap_or_default();
        let files = git::run(&d, &["ls-files"]).unwrap_or_default();
        let times = git::run(&d, &["log", "--since=12.weeks", "--format=%ct"]).unwrap_or_default();
        (total, authors, first.lines().next().unwrap_or("").to_string(), files, times)
    });
    let file_list: Vec<&str> = files.lines().collect();
    ui::kv("Коммитов", &c(CYAN, &total));
    ui::kv("Первый коммит", &first);
    ui::kv("Файлов в git", &file_list.len().to_string());
    // активность по неделям
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut weeks = [0u32; 12];
    for t in times.lines().filter_map(|l| l.parse::<u64>().ok()) {
        let w = (now.saturating_sub(t) / (7 * 86400)) as usize;
        if w < 12 {
            weeks[11 - w] += 1;
        }
    }
    let mx = *weeks.iter().max().unwrap_or(&1).max(&1);
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let spark: String = weeks
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let ch = if v == 0 { '·' } else { bars[((v as f32 / mx as f32) * 7.0).round() as usize] };
            format!("{}{} ", fg(ui::grad(i as f32 / 11.0)), ch)
        })
        .collect();
    ui::kv("Активность, 12 нед.", &format!("{}{}", spark, fg(PRIMARY)));
    ui::blank();
    ui::title("Языки и типы файлов");
    let mut by_ext: std::collections::HashMap<String, (usize, u64)> = Default::default();
    for f in &file_list {
        let p = Path::new(f);
        let e = p.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_else(|| "(без расширения)".into());
        let sz = std::fs::metadata(d.join(f)).map(|m| m.len()).unwrap_or(0);
        let ent = by_ext.entry(e).or_default();
        ent.0 += 1;
        ent.1 += sz;
    }
    let mut v: Vec<(String, (usize, u64))> = by_ext.into_iter().collect();
    v.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
    let tot: u64 = v.iter().map(|x| x.1 .1).sum::<u64>().max(1);
    for (e, (n, sz)) in v.iter().take(8) {
        let frac = *sz as f32 / tot as f32;
        ui::line(&format!("{}   {}", ui::progress_bar(&format!(".{} ({} ф.)", e.trim_start_matches('.'), n), frac, 28), c(DIM, &crate::gitignore::human(*sz))));
        ui::frame(40);
    }
    ui::blank();
    ui::title("Авторы");
    let rows: Vec<(u32, String)> = authors
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (n, name) = l.split_once('\t')?;
            Some((n.trim().parse().ok()?, name.to_string()))
        })
        .collect();
    let am = rows.iter().map(|r| r.0).max().unwrap_or(1);
    for (n, name) in rows.iter().take(6) {
        ui::line(&format!(
            "{}{}",
            ui::progress_bar(&ui::truncate(name, 24), *n as f32 / am as f32, 28),
            c(DIM, &format!("  {} коммитов", n))
        ));
        ui::frame(40);
    }
    ui::pause();
}

// ───────────────────── клонирование ─────────────────────

pub fn clone_repo(app: &mut App) {
    if !auth::require(app) {
        return;
    }
    ui::header("Клонировать репозиторий");
    let tok = app.token.clone().unwrap();
    let repos = match ui::spin("Загружаю список твоих репозиториев", || github::list_repos(&tok)) {
        Ok(r) => r,
        Err(e) => {
            ui::err(&e);
            ui::pause();
            return;
        }
    };
    let mut items = vec![Item::new("⌨", "Ввести URL вручную", "любой публичный репозиторий")];
    items.push(Item::sep(&format!("Мои репозитории ({})", repos.len())));
    for r in &repos {
        let lock = if r.private { "⚿" } else { "◎" };
        let hint = format!("{} {} {}", r.language, if r.stars > 0 { format!("★{}", r.stars) } else { String::new() }, r.updated);
        items.push(Item::new(lock, &r.full_name, &hint));
    }
    let Some(ch) = ui::menu(&items, 0) else { return };
    ui::blank();
    let url = if ch == 0 {
        match ui::input_req("URL репозитория", "https://github.com/") {
            Some(u) => u,
            None => return,
        }
    } else {
        repos[ch - 2].clone_url.clone()
    };
    let name = git::github_slug(&url).map(|(_, r)| r).unwrap_or_else(|| "repo".into());
    ui::info("Куда склонировать?");
    let default_parent = dirs::document_dir().unwrap_or_default().join("GitHub");
    let parent = match ui::choose("Папка назначения", &[&format!("{}", default_parent.to_string_lossy()), "Выбрать в проводнике…"]) {
        Some(0) => default_parent,
        Some(1) => match rfd::FileDialog::new().set_title("Куда склонировать").pick_folder() {
            Some(p) => p,
            None => return,
        },
        _ => return,
    };
    let _ = std::fs::create_dir_all(&parent);
    let target = parent.join(&name);
    if target.exists() {
        ui::warn(&format!("Папка {} уже существует", target.to_string_lossy()));
        ui::pause();
        return;
    }
    let login = app.login.clone().unwrap_or_default();
    let t = target.to_string_lossy().to_string();
    let auth = if url.contains("github.com") { Some((login.as_str(), tok.as_str())) } else { None };
    let r = ui::spin(&format!("Клонирую {}", name), || git::run_net(&parent, &["clone", &url, &t], auth));
    if report(r, &format!("Склонировано в {}", t)) {
        project::set_project(app, PathBuf::from(&t));
        ui::info("Проект выбран как текущий.");
    }
    ui::pause();
}

// ───────────────────── браузер ─────────────────────

pub fn browser(app: &mut App) {
    let Some(d) = need_repo(app) else { return };
    let Some((o, r)) = git::remote_url(&d).and_then(|u| git::github_slug(&u)) else {
        ui::header("Открыть на GitHub");
        ui::warn("Проект не подключён к GitHub. Сначала опубликуй его.");
        ui::pause();
        return;
    };
    let base = format!("https://github.com/{}/{}", o, r);
    let br = git::branch(&d);
    ui::header("Открыть на GitHub");
    ui::kv("Репозиторий", &c(CYAN, &base));
    ui::blank();
    let items = vec![
        Item::new("◎", "Страница репозитория", ""),
        Item::new("⇄", "Создать Pull Request из текущей ветки", &br),
        Item::new("!", "Issues", ""),
        Item::new("⇋", "Pull Requests", ""),
        Item::new("▶", "Actions (CI)", ""),
        Item::new("★", "Releases", ""),
        Item::new("⚙", "Настройки репозитория", ""),
    ];
    let urls = [
        base.clone(),
        format!("{}/compare/{}?expand=1", base, br),
        format!("{}/issues", base),
        format!("{}/pulls", base),
        format!("{}/actions", base),
        format!("{}/releases", base),
        format!("{}/settings", base),
    ];
    if let Some(i) = ui::menu(&items, 0) {
        let _ = open::that(&urls[i]);
    }
}

// ───────────────────── настройки ─────────────────────

pub fn settings(app: &mut App) {
    loop {
        ui::header("Настройки");
        let anim = ["выкл", "быстрые", "плавные"][app.cfg.anim as usize];
        let onoff = |b: bool| if b { "вкл" } else { "выкл" };
        let items = vec![
            Item::new("✦", "Анимации", anim),
            Item::new("⚿", "Новые репозитории по умолчанию", if app.cfg.default_private { "приватные" } else { "публичные" }),
            Item::new("⎇", "Имя основной ветки", &app.cfg.default_branch),
            Item::new("↗", "Открывать браузер после публикации", onoff(app.cfg.auto_open_browser)),
            Item::new("⛨", "Проверка на секреты перед коммитом", onoff(app.cfg.scan_secrets)),
            Item::new("☺", "Имя и почта для коммитов", &format!("{} <{}>", git::config_get("user.name").unwrap_or("—".into()), git::config_get("user.email").unwrap_or("—".into()))),
            Item::new("⬇", "Проверить/обновить Git", ""),
            Item::new("✕", "Очистить список недавних проектов", &format!("{} шт.", app.cfg.recents.len())),
        ];
        let Some(ch) = ui::menu(&items, 0) else { return };
        ui::blank();
        match ch {
            0 => {
                app.cfg.anim = (app.cfg.anim + 1) % 3;
                ui::set_anim(app.cfg.anim);
            }
            1 => app.cfg.default_private = !app.cfg.default_private,
            2 => {
                if let Some(b) = ui::input_req("Имя ветки", &app.cfg.default_branch) {
                    app.cfg.default_branch = b;
                }
            }
            3 => app.cfg.auto_open_browser = !app.cfg.auto_open_browser,
            4 => app.cfg.scan_secrets = !app.cfg.scan_secrets,
            5 => {
                let n = ui::input_req("Имя", &git::config_get("user.name").unwrap_or_default());
                let m = n.as_ref().and_then(|_| ui::input_req("Почта", &git::config_get("user.email").unwrap_or_default()));
                if let (Some(n), Some(m)) = (n, m) {
                    let _ = git::run_global(&["config", "--global", "user.name", &n]);
                    let _ = git::run_global(&["config", "--global", "user.email", &m]);
                    ui::success("Сохранено");
                    ui::pause();
                }
            }
            6 => {
                install_git(app, true);
            }
            7 => app.cfg.recents.clear(),
            _ => {}
        }
        app.cfg.save();
    }
}

// ───────────────────── установка Git ─────────────────────

pub fn install_git(app: &mut App, update: bool) -> bool {
    ui::header(if update { "Обновление Git" } else { "Установка Git" });
    if !update {
        ui::boxed(
            &[
                c(WARN, "Git не найден на этом компьютере"),
                String::new(),
                c(ICE, "Без него GitHelper не сможет делать коммиты и публиковать проекты."),
                c(ICE, "Установка займёт пару минут: Git for Windows + Credential Manager."),
            ],
            WARN,
        );
        ui::blank();
    } else if let Some(v) = &app.git_ver {
        ui::kv("Установлен Git", v);
        ui::blank();
    }
    let has_winget = std::process::Command::new("winget").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    let mut opts = vec![];
    if has_winget {
        opts.push(if update { "Обновить через winget (рекомендую)" } else { "Установить автоматически через winget (рекомендую)" });
    }
    opts.push("Открыть страницу загрузки git-scm.com");
    opts.push(if update { "Назад" } else { "Пропустить" });
    let Some(ch) = ui::choose("Как поступим?", &opts) else { return false };
    let ch = if has_winget { ch } else { ch + 1 };
    ui::blank();
    match ch {
        0 => {
            let verb = if update { "upgrade" } else { "install" };
            ui::info("Запускаю установщик — Windows может спросить разрешение (UAC).");
            ui::blank();
            let status = ui::cooked(|| {
                std::process::Command::new("winget")
                    .args([verb, "--id", "Git.Git", "-e", "--source", "winget", "--accept-package-agreements", "--accept-source-agreements"])
                    .status()
            });
            ui::blank();
            match status {
                Ok(_) => {
                    if let Some(v) = ui::spin("Проверяю установку", git::detect) {
                        app.git_ver = Some(v.clone());
                        ui::success(&format!("Git {} готов к работе", v));
                        ui::pause();
                        return true;
                    }
                    ui::warn("Git пока не виден — возможно, нужно перезапустить GitHelper.");
                }
                Err(e) => ui::err(&format!("Не удалось запустить winget: {}", e)),
            }
            ui::pause();
            false
        }
        1 => {
            let _ = open::that("https://git-scm.com/download/win");
            ui::info("Скачай и установи Git (все настройки по умолчанию подходят), затем вернись сюда.");
            ui::pause();
            if let Some(v) = git::detect() {
                app.git_ver = Some(v);
                ui::success("Git найден!");
                ui::pause();
                return true;
            }
            false
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn _unused() {
    let _ = (DIM, ERR, FAINT, OK, WHITE);
}
