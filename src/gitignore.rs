//! Мастер .gitignore: автоопределение стека, шаблоны, выбор файлов в дереве.

use crate::git;
use crate::github;
use crate::ui::{self, c, fg, Check, Item, CYAN, DIM, FAINT, ICE, OK, PRIMARY, WARN, WHITE};
use crossterm::event::KeyCode;
use std::path::{Path, PathBuf};

pub struct Stack {
    pub name: &'static str,
    pub markers: &'static [&'static str],
    pub body: &'static str,
}

pub const STACKS: &[Stack] = &[
    Stack { name: "Windows / macOS / Linux (системный мусор)", markers: &[], body: "Thumbs.db\nehthumbs.db\nDesktop.ini\n$RECYCLE.BIN/\n.DS_Store\n.AppleDouble\n._*\n*~\n.directory\n" },
    Stack { name: "Rust", markers: &["Cargo.toml"], body: "/target/\n**/*.rs.bk\n*.pdb\n" },
    Stack { name: "Node.js / JavaScript / TypeScript", markers: &["package.json"], body: "node_modules/\nnpm-debug.log*\nyarn-debug.log*\nyarn-error.log*\npnpm-debug.log*\n.npm/\n.yarn/cache\ndist/\nbuild/\n.next/\n.nuxt/\nout/\ncoverage/\n.cache/\n.parcel-cache/\n*.tsbuildinfo\n.eslintcache\n" },
    Stack { name: "Python", markers: &["requirements.txt", "pyproject.toml", "setup.py", "Pipfile", "*.py"], body: "__pycache__/\n*.py[cod]\n*$py.class\n*.egg-info/\n.eggs/\ndist/\nbuild/\n.venv/\nvenv/\nenv/\n.pytest_cache/\n.mypy_cache/\n.ruff_cache/\n.ipynb_checkpoints/\n.coverage\nhtmlcov/\n" },
    Stack { name: "C# / .NET / Visual Studio", markers: &["*.sln", "*.csproj", "*.vbproj", "*.fsproj"], body: "[Bb]in/\n[Oo]bj/\n.vs/\n*.user\n*.suo\n*.userprefs\n[Dd]ebug/\n[Rr]elease/\nx64/\nx86/\n*.nupkg\npackages/\nTestResults/\n*.DotSettings.user\n" },
    Stack { name: "Java / Kotlin (Gradle, Maven)", markers: &["pom.xml", "build.gradle", "build.gradle.kts", "settings.gradle"], body: "*.class\n*.jar\n*.war\n*.ear\ntarget/\n.gradle/\nbuild/\nout/\n!gradle/wrapper/gradle-wrapper.jar\nhs_err_pid*\n" },
    Stack { name: "Go", markers: &["go.mod"], body: "*.exe\n*.exe~\n*.dll\n*.so\n*.dylib\n*.test\n*.out\nvendor/\ngo.work\n" },
    Stack { name: "C / C++ / CMake", markers: &["CMakeLists.txt", "Makefile", "*.cpp", "*.c"], body: "build/\ncmake-build-*/\nCMakeFiles/\nCMakeCache.txt\n*.o\n*.obj\n*.a\n*.lib\n*.so\n*.dll\n*.exe\n*.pdb\n*.ilk\n" },
    Stack { name: "Unity", markers: &["ProjectSettings", "Assets"], body: "/[Ll]ibrary/\n/[Tt]emp/\n/[Oo]bj/\n/[Bb]uild/\n/[Bb]uilds/\n/[Ll]ogs/\n/[Uu]ser[Ss]ettings/\n/[Mm]emoryCaptures/\n*.csproj\n*.unityproj\n*.sln\n*.pidb\n*.booproj\n*.svd\n*.pdb\n*.mdb\n*.opendb\n*.VC.db\n*.apk\n*.aab\n*.unitypackage\ncrashlytics-build.properties\n" },
    Stack { name: "Godot", markers: &["project.godot"], body: ".godot/\n.import/\nexport.cfg\nexport_presets.cfg\n*.translation\n.mono/\ndata_*/\nmono_crash.*.json\n" },
    Stack { name: "PHP / Laravel / Composer", markers: &["composer.json"], body: "/vendor/\n/node_modules/\n/public/hot\n/public/storage\n/storage/*.key\n.phpunit.result.cache\nHomestead.json\nHomestead.yaml\n" },
    Stack { name: "Flutter / Dart", markers: &["pubspec.yaml"], body: ".dart_tool/\n.packages\n.pub-cache/\n.pub/\nbuild/\n.flutter-plugins\n.flutter-plugins-dependencies\n" },
    Stack { name: "VS Code", markers: &[".vscode"], body: ".vscode/*\n!.vscode/settings.json\n!.vscode/tasks.json\n!.vscode/launch.json\n!.vscode/extensions.json\n*.code-workspace\n" },
    Stack { name: "JetBrains (IDEA, Rider, PyCharm…)", markers: &[".idea"], body: ".idea/\n*.iml\n*.iws\nout/\n" },
    Stack { name: "Секреты и окружение (.env, ключи)", markers: &[], body: ".env\n.env.*\n!.env.example\n*.pem\n*.key\n*.p12\n*.pfx\nid_rsa*\nsecrets.json\ncredentials.json\n" },
    Stack { name: "Логи и временные файлы", markers: &[], body: "*.log\nlogs/\n*.tmp\n*.temp\n*.swp\n*.bak\n" },
];

/// Определяет стеки по маркерам в корне проекта (и на один уровень глубже).
pub fn detect(dir: &Path) -> Vec<usize> {
    let mut names: Vec<String> = vec![];
    let mut scan = |d: &Path| {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                names.push(e.file_name().to_string_lossy().to_string());
            }
        }
    };
    scan(dir);
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() && !n.starts_with('.') && !matches!(n.as_str(), "node_modules" | "target" | "bin" | "obj") {
                scan(&e.path());
            }
        }
    }
    let mut res = vec![0usize]; // системный мусор всегда полезен
    for (i, s) in STACKS.iter().enumerate() {
        let hit = s.markers.iter().any(|m| {
            if let Some(ext) = m.strip_prefix("*.") {
                names.iter().any(|n| n.to_lowercase().ends_with(&format!(".{}", ext)))
            } else {
                names.iter().any(|n| n == m)
            }
        });
        if hit {
            res.push(i);
        }
    }
    // Unity: нужны обе папки
    let unity = STACKS.iter().position(|s| s.name == "Unity").unwrap();
    if res.contains(&unity) && !(dir.join("Assets").is_dir() && dir.join("ProjectSettings").is_dir()) {
        res.retain(|&i| i != unity);
    }
    res.push(STACKS.len() - 2); // секреты
    res.dedup();
    res
}

// ───────────────────── Дерево файлов ─────────────────────

const SUGGEST: &[&str] = &[
    "node_modules", "target", "bin", "obj", "dist", "build", "out", ".venv", "venv", "__pycache__", ".idea", ".vs",
    ".env", ".next", ".nuxt", "coverage", ".cache", "Library", "Temp", "Logs", ".gradle", ".pytest_cache", ".mypy_cache",
    "Thumbs.db", ".DS_Store", ".dart_tool", "vendor", ".godot",
];

struct Node {
    path: PathBuf,
    rel: String,
    name: String,
    depth: usize,
    dir: bool,
    open: bool,
    loaded: bool,
    on: bool,
    size: u64,
    suggested: bool,
}

fn children(root: &Path, p: &Path, depth: usize) -> Vec<Node> {
    let mut v: Vec<Node> = std::fs::read_dir(p)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_name() != ".git")
                .map(|e| {
                    let path = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    let dir = path.is_dir();
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let size = if dir { 0 } else { e.metadata().map(|m| m.len()).unwrap_or(0) };
                    let suggested = SUGGEST.iter().any(|s| s.eq_ignore_ascii_case(&name))
                        || name.ends_with(".log")
                        || (name.starts_with(".env") && name != ".env.example");
                    Node { path, rel, name, depth, dir, open: false, loaded: false, on: suggested, size, suggested }
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.dir.cmp(&a.dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

pub fn human(n: u64) -> String {
    let f = n as f64;
    if f >= 1073741824.0 {
        format!("{:.1} ГБ", f / 1073741824.0)
    } else if f >= 1048576.0 {
        format!("{:.1} МБ", f / 1048576.0)
    } else if f >= 1024.0 {
        format!("{:.0} КБ", f / 1024.0)
    } else {
        format!("{} Б", n)
    }
}

/// Интерактивное дерево: выбираешь, какие файлы и папки игнорировать.
/// Возвращает строки для .gitignore (или None при отмене).
pub fn pick_tree(root: &Path) -> Option<Vec<String>> {
    let mut nodes = children(root, root, 0);
    let mut cur = 0usize;
    let mut scroll = 0usize;
    ui::line(&format!("  {} {}", c(CYAN, "?"), c(WHITE, "Отметь файлы и папки, которые НЕ должны попасть в репозиторий")));
    ui::dim("  Подсказки уже отмечены (node_modules, target, .env, логи…). Отметка папки = игнор всей папки.");
    ui::blank();
    let (_, top) = crossterm::cursor::position().unwrap_or((0, 7));
    let (w, h) = ui::size();
    let avail = (h as usize).saturating_sub(top as usize + 4).max(5);
    let mut first = true;
    loop {
        if nodes.is_empty() {
            ui::warn("Папка пуста");
            return Some(vec![]);
        }
        cur = cur.min(nodes.len() - 1);
        if cur < scroll {
            scroll = cur;
        }
        if cur >= scroll + avail {
            scroll = cur + 1 - avail;
        }
        let mut buf = String::new();
        for row in 0..avail {
            buf.push_str(&format!("{}{}\x1b[K", ui::goto(0, top + row as u16), ui::bg(ui::BG)));
            let i = scroll + row;
            if i >= nodes.len() {
                continue;
            }
            let n = &nodes[i];
            // если родитель отмечен — показываем как «унаследовано»
            let inherited = nodes.iter().any(|p| p.dir && p.on && n.rel.starts_with(&format!("{}/", p.rel)));
            let mark = if n.on {
                c(CYAN, "◉")
            } else if inherited {
                c(DIM, "◌")
            } else {
                c(FAINT, "○")
            };
            let arrow = if n.dir { if n.open { "▾" } else { "▸" } } else { " " };
            let icon = if n.dir { c(ICE, "▣") } else { c(DIM, "▫") };
            let indent = "  ".repeat(n.depth);
            let name = ui::truncate(&n.name, (w as usize).saturating_sub(40 + n.depth * 2).max(12));
            let meta = if n.dir { String::new() } else { human(n.size) };
            let tag = if n.suggested { c(WARN, " ★ рекомендую") } else { String::new() };
            let nm_col = if n.on || inherited { DIM } else if n.dir { ICE } else { PRIMARY };
            if i == cur {
                buf.push_str(&format!(
                    "  {}{}▌{} {} {}{} {}{}{}  {}{}{}{}",
                    ui::bg(ui::SEL_BG),
                    fg(CYAN),
                    indent,
                    mark,
                    fg(CYAN),
                    arrow,
                    icon,
                    fg(WHITE),
                    format!(" {}", name),
                    fg(DIM),
                    meta,
                    tag,
                    ui::bg(ui::BG)
                ));
            } else {
                buf.push_str(&format!(
                    "   {} {} {}{} {}{} {}{}{}",
                    indent,
                    mark,
                    fg(DIM),
                    arrow,
                    icon,
                    fg(nm_col),
                    name,
                    c(FAINT, &format!("  {}", meta)),
                    tag
                ));
            }
            if first && ui::anim_on() && row < 30 {
                ui::out(&buf);
                buf.clear();
                ui::frame(7);
            }
        }
        first = false;
        let cnt = nodes.iter().filter(|n| n.on).count();
        buf.push_str(&format!(
            "{}{}  {}отмечено: {}{}{}\x1b[K",
            ui::goto(0, top + avail as u16),
            ui::bg(ui::BG),
            fg(DIM),
            fg(CYAN),
            cnt,
            fg(PRIMARY)
        ));
        buf.push_str(&format!(
            "{}{}  {}Пробел{} отметить  {}→/←{} раскрыть/свернуть  {}R{} только рекомендации  {}Enter{} готово  {}Esc{} отмена{}\x1b[K",
            ui::goto(0, top + avail as u16 + 1),
            ui::bg(ui::BG),
            fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(PRIMARY)
        ));
        ui::out(&buf);

        let k = ui::read_key();
        match k.code {
            KeyCode::Up => cur = if cur == 0 { nodes.len() - 1 } else { cur - 1 },
            KeyCode::Down => cur = (cur + 1) % nodes.len(),
            KeyCode::PageUp => cur = cur.saturating_sub(avail),
            KeyCode::PageDown => cur = (cur + avail).min(nodes.len() - 1),
            KeyCode::Char(' ') => nodes[cur].on = !nodes[cur].on,
            KeyCode::Char('r') | KeyCode::Char('к') => {
                for n in nodes.iter_mut() {
                    n.on = n.suggested;
                }
            }
            KeyCode::Right => {
                if nodes[cur].dir && !nodes[cur].open {
                    nodes[cur].open = true;
                    if !nodes[cur].loaded {
                        nodes[cur].loaded = true;
                    }
                    let kids = children(root, &nodes[cur].path.clone(), nodes[cur].depth + 1);
                    let at = cur + 1;
                    for (j, kid) in kids.into_iter().enumerate() {
                        nodes.insert(at + j, kid);
                    }
                } else if nodes[cur].dir && cur + 1 < nodes.len() {
                    cur += 1;
                }
            }
            KeyCode::Left => {
                if nodes[cur].dir && nodes[cur].open {
                    let d = nodes[cur].depth;
                    let mut end = cur + 1;
                    while end < nodes.len() && nodes[end].depth > d {
                        end += 1;
                    }
                    nodes.drain(cur + 1..end);
                    nodes[cur].open = false;
                } else if nodes[cur].depth > 0 {
                    let d = nodes[cur].depth;
                    while cur > 0 && nodes[cur].depth >= d {
                        cur -= 1;
                    }
                }
            }
            KeyCode::Enter => {
                ui::out(&format!("{}\r\n", ui::goto(0, top + avail as u16 + 2)));
                let mut res = vec![];
                for n in nodes.iter().filter(|n| n.on) {
                    // не дублируем, если родитель уже игнорируется
                    if nodes.iter().any(|p| p.dir && p.on && n.rel.starts_with(&format!("{}/", p.rel))) {
                        continue;
                    }
                    if n.depth == 0 && n.dir {
                        res.push(format!("/{}/", n.rel));
                    } else if n.dir {
                        res.push(format!("/{}/", n.rel));
                    } else {
                        res.push(format!("/{}", n.rel));
                    }
                }
                return Some(res);
            }
            KeyCode::Esc => {
                ui::out(&format!("{}\r\n", ui::goto(0, top + avail as u16 + 2)));
                return None;
            }
            _ => {}
        }
    }
}

// ───────────────────── Сборка файла ─────────────────────

fn build(stacks: &[usize], extra: &[(String, String)], picked: &[String]) -> String {
    let mut s = String::from("# ─────────────────────────────────────────────\n#  .gitignore — создан GitHelper\n# ─────────────────────────────────────────────\n");
    for &i in stacks {
        s.push_str(&format!("\n# ── {} ──\n{}", STACKS[i].name, STACKS[i].body));
    }
    for (name, body) in extra {
        s.push_str(&format!("\n# ── {} (шаблон GitHub) ──\n{}\n", name, body.trim_end()));
    }
    if !picked.is_empty() {
        s.push_str("\n# ── Выбрано вручную ──\n");
        for p in picked {
            s.push_str(p);
            s.push('\n');
        }
    }
    s
}

/// Убирает дубли строк при слиянии с существующим .gitignore.
fn merge(existing: &str, new: &str) -> String {
    let have: std::collections::HashSet<String> =
        existing.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    let mut out = existing.trim_end().to_string();
    out.push_str("\n\n# ── Добавлено GitHelper ──\n");
    let mut added = 0;
    for l in new.lines() {
        let t = l.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if !have.contains(t) && !out.lines().any(|x| x.trim() == t) {
            out.push_str(t);
            out.push('\n');
            added += 1;
        }
    }
    if added == 0 {
        existing.to_string()
    } else {
        out
    }
}

fn colorize(content: &str) -> Vec<String> {
    content
        .lines()
        .enumerate()
        .map(|(i, l)| {
            let num = c(FAINT, &format!("{:>4} ", i + 1));
            let body = if l.starts_with('#') {
                c(DIM, l)
            } else if l.starts_with('!') {
                c(OK, l)
            } else if l.ends_with('/') {
                c(ICE, l)
            } else {
                c(PRIMARY, l)
            };
            format!("{}{}", num, body)
        })
        .collect()
}

/// Создание нового содержимого: стек + шаблоны GitHub + дерево.
fn generate(dir: &Path) -> Option<String> {
    let detected = detect(dir);
    let mut checks: Vec<Check> = STACKS
        .iter()
        .enumerate()
        .map(|(i, s)| Check {
            label: s.name.to_string(),
            hint: if detected.contains(&i) && !s.markers.is_empty() { "найдено в проекте".into() } else { String::new() },
            on: detected.contains(&i),
            color: PRIMARY,
        })
        .collect();
    checks.push(Check { label: "＋ Добавить шаблон из каталога GitHub (200+ шаблонов)…".into(), hint: String::new(), on: false, color: CYAN });
    ui::header(".gitignore › шаблоны");
    if !detected.is_empty() {
        let found: Vec<&str> = detected.iter().filter(|&&i| !STACKS[i].markers.is_empty()).map(|&i| STACKS[i].name).collect();
        if !found.is_empty() {
            ui::success(&format!("Определил стек: {}", found.join(", ")));
            ui::blank();
        }
    }
    if !ui::checklist("Какие шаблоны включить?", &mut checks) {
        return None;
    }
    let stacks: Vec<usize> = (0..STACKS.len()).filter(|&i| checks[i].on).collect();
    let mut extra = vec![];
    if checks[STACKS.len()].on {
        ui::header(".gitignore › каталог GitHub");
        match ui::spin("Загружаю каталог шаблонов GitHub", github::gitignore_list) {
            Ok(list) => {
                let mut ch: Vec<Check> =
                    list.iter().map(|n| Check { label: n.clone(), hint: String::new(), on: false, color: PRIMARY }).collect();
                if ui::checklist("Выбери шаблоны (/ — поиск)", &mut ch) {
                    for (i, n) in list.iter().enumerate().filter(|(i, _)| ch[*i].on) {
                        let _ = i;
                        match ui::spin(&format!("Скачиваю {}", n), || github::gitignore_get(n)) {
                            Ok(b) => extra.push((n.clone(), b)),
                            Err(e) => ui::err(&e),
                        }
                    }
                }
            }
            Err(e) => {
                ui::err(&e);
                ui::pause();
            }
        }
    }
    ui::header(".gitignore › выбор файлов");
    let picked = if ui::confirm("Выбрать конкретные файлы/папки в дереве проекта?", true) {
        ui::blank();
        pick_tree(dir)?
    } else {
        vec![]
    };
    Some(build(&stacks, &extra, &picked))
}

/// Главный мастер .gitignore. Возвращает false, если пользователь отменил.
pub fn wizard(dir: &Path) -> bool {
    let path = dir.join(".gitignore");
    let exists = path.exists();
    ui::header(".gitignore");
    let content = if exists {
        let cur = std::fs::read_to_string(&path).unwrap_or_default();
        let rules = cur.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count();
        ui::info(&format!("В проекте уже есть .gitignore — {} правил", c(CYAN, &rules.to_string())));
        ui::blank();
        let items = [
            Item::new("✔", "Использовать существующий", "ничего не менять"),
            Item::new("＋", "Дополнить", "сгенерировать и добавить недостающие правила"),
            Item::new("↻", "Заменить новым", "создать заново"),
            Item::new("▤", "Посмотреть", "открыть текущий файл"),
            Item::new("✎", "Добавить правило вручную", "одна строка"),
        ];
        loop {
            match ui::menu(&items, 0) {
                Some(0) => return true,
                Some(1) => match generate(dir) {
                    Some(n) => break merge(&cur, &n),
                    None => return false,
                },
                Some(2) => match generate(dir) {
                    Some(n) => break n,
                    None => return false,
                },
                Some(3) => {
                    ui::pager(".gitignore", &colorize(&cur));
                    ui::header(".gitignore");
                }
                Some(4) => {
                    ui::blank();
                    if let Some(r) = ui::input("Правило (например *.log или /secret/)", "", false) {
                        if !r.is_empty() {
                            let mut n = cur.trim_end().to_string();
                            n.push('\n');
                            n.push_str(&r);
                            n.push('\n');
                            break n;
                        }
                    }
                    ui::header(".gitignore");
                }
                _ => return false,
            }
        }
    } else {
        let items = [
            Item::new("✦", "Сгенерировать .gitignore", "автоопределение стека + выбор файлов"),
            Item::new("▣", "Только выбрать файлы в дереве", "без шаблонов"),
            Item::new("→", "Без .gitignore", "закоммитить всё как есть"),
        ];
        ui::info("В проекте нет .gitignore");
        ui::blank();
        match ui::menu(&items, 0) {
            Some(0) => match generate(dir) {
                Some(n) => n,
                None => return false,
            },
            Some(1) => {
                ui::header(".gitignore › выбор файлов");
                match pick_tree(dir) {
                    Some(p) => build(&[0], &[], &p),
                    None => return false,
                }
            }
            Some(2) => return true,
            _ => return false,
        }
    };

    // предпросмотр и запись
    ui::pager(".gitignore › предпросмотр (Esc — дальше)", &colorize(&content));
    ui::header(".gitignore › сохранение");
    let rules = content.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count();
    ui::info(&format!("Правил в файле: {}", c(CYAN, &rules.to_string())));
    ui::blank();
    if !ui::confirm("Сохранить .gitignore?", true) {
        return false;
    }
    if let Err(e) = std::fs::write(&path, &content) {
        ui::err(&format!("Не удалось записать: {}", e));
        ui::pause();
        return false;
    }
    ui::success(".gitignore сохранён");
    untrack_ignored(dir);
    true
}

/// Файлы, которые уже в git, но теперь попадают под .gitignore — предлагаем убрать их из индекса.
pub fn untrack_ignored(dir: &Path) {
    if !git::is_repo(dir) {
        return;
    }
    let list = git::run(dir, &["ls-files", "-ci", "--exclude-standard"]).unwrap_or_default();
    let files: Vec<&str> = list.lines().filter(|l| !l.is_empty()).collect();
    if files.is_empty() {
        return;
    }
    ui::blank();
    ui::warn(&format!("{} файл(ов) уже отслеживаются git, но теперь игнорируются:", files.len()));
    for f in files.iter().take(8) {
        ui::dim(&format!("   {}", f));
    }
    if files.len() > 8 {
        ui::dim(&format!("   … и ещё {}", files.len() - 8));
    }
    if ui::confirm("Убрать их из репозитория (файлы на диске останутся)?", true) {
        let mut args = vec!["rm", "-r", "--cached", "--quiet", "--"];
        args.extend(files.iter().copied());
        match git::run(dir, &args) {
            Ok(_) => ui::success("Убраны из индекса — закоммить изменения, чтобы применить"),
            Err(e) => ui::err(&e),
        }
    }
}
