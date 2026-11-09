//! GitHelper — консольный помощник для Git и GitHub в стиле "color 9".

mod auth;
mod config;
mod git;
mod github;
mod gitignore;
mod project;
mod tools;
mod ui;

use std::path::PathBuf;
use ui::{c, Item, CYAN, DIM, ICE, OK, WARN, WHITE};

pub struct App {
    pub cfg: config::Config,
    pub token: Option<String>,
    pub login: Option<String>,
    pub project: Option<PathBuf>,
    pub git_ver: Option<String>,
}

type Action = fn(&mut App);

fn main() {
    // Ctrl+C / паника — вернуть терминал в нормальное состояние
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ui::shutdown();
        default_hook(info);
    }));

    let cfg = config::Config::load();
    ui::set_anim(cfg.anim);
    let mut app = App { cfg, token: None, login: None, project: None, git_ver: None };

    // папка из аргумента командной строки: GitHelper.exe C:\path\to\project
    let arg_dir = std::env::args().nth(1).map(PathBuf::from).filter(|p| p.is_dir());

    ui::init();
    boot(&mut app);

    if app.git_ver.is_none() {
        tools::install_git(&mut app, false);
    }
    if !app.cfg.first_run_done {
        first_run(&mut app);
    }
    if let Some(d) = arg_dir {
        project::set_project(&mut app, d);
    } else if let Some(r) = app.cfg.recents.first().cloned() {
        if std::path::Path::new(&r).is_dir() {
            app.project = Some(PathBuf::from(r));
        }
    }

    main_menu(&mut app);
    goodbye();
    ui::shutdown();
}

/// Заставка + проверки при запуске.
fn boot(app: &mut App) {
    let steps = ["Запуск интерфейса", "Поиск Git", "Учётная запись", "Связь с GitHub"];
    let mut token: Option<String> = None;
    let mut git_ver: Option<String> = None;
    let mut login: Option<String> = None;
    let saved_login = app.cfg.login.clone();
    let mut check = |i: usize| -> (String, bool) {
        match i {
            0 => ("Интерфейс готов · тема color 9".into(), true),
            1 => match git::detect() {
                Some(v) => {
                    git_ver = Some(v.clone());
                    (format!("Git {}", v), true)
                }
                None => ("Git не найден — предложу установить".into(), false),
            },
            2 => match (config::load_token(), &saved_login) {
                (Some(t), Some(l)) => {
                    token = Some(t);
                    login = Some(l.clone());
                    (format!("Вход выполнен: @{}", l), true)
                }
                _ => ("Вход в GitHub не выполнен".into(), false),
            },
            _ => {
                if github::ping() {
                    ("GitHub доступен".into(), true)
                } else {
                    ("Нет связи с GitHub — работаю офлайн".into(), false)
                }
            }
        }
    };
    ui::splash(&mut check, &steps);
    app.git_ver = git_ver;
    app.token = token;
    app.login = login;
}

fn first_run(app: &mut App) {
    ui::header("Добро пожаловать");
    ui::typewriter("Привет! Я GitHelper — помогу сохранять проекты в Git и публиковать их на GitHub.", WHITE);
    ui::typewriter("Без запоминания команд: выбираешь папку, отмечаешь файлы — и готово.", ICE);
    ui::blank();
    ui::boxed(
        &[
            c(CYAN, "Что я умею:"),
            format!("{} Публикация проекта на GitHub в один мастер", c(OK, "✔")),
            format!("{} Умный .gitignore: автоопределение стека + выбор файлов", c(OK, "✔")),
            format!("{} Защита от утечки паролей, токенов и ключей в коммитах", c(OK, "✔")),
            format!("{} Ветки, история, stash, откат ошибок, релизы, статистика", c(OK, "✔")),
            format!("{} Вход по почте и паролю или по токену", c(OK, "✔")),
        ],
        ui::DEEP,
    );
    ui::blank();
    if app.token.is_none() && app.git_ver.is_some() && ui::confirm("Войти в GitHub сейчас?", true) {
        auth::menu(app);
    }
    app.cfg.first_run_done = true;
    app.cfg.save();
}

fn refresh_status(app: &App) {
    let mut st = vec![];
    st.push((
        "аккаунт".to_string(),
        app.login.as_ref().map(|l| format!("@{}", l)).unwrap_or_else(|| c(WARN, "не выполнен вход")),
    ));
    if let Some(p) = &app.project {
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        st.push(("проект".into(), name));
        if git::is_repo(p) {
            let br = git::branch(p);
            let n = git::status(p).len();
            st.push(("ветка".into(), format!("⎇ {}", if br.is_empty() { "—".into() } else { br })));
            st.push((
                "изменений".into(),
                if n == 0 { c(OK, "0 ✔") } else { c(WARN, &n.to_string()) },
            ));
        } else {
            st.push(("git".into(), c(DIM, "нет репозитория")));
        }
    } else {
        st.push(("проект".into(), c(DIM, "не выбран")));
    }
    if let Some(v) = &app.git_ver {
        st.push(("git".into(), v.split(".windows").next().unwrap_or(v).to_string()));
    }
    ui::set_status(st);
}

fn main_menu(app: &mut App) {
    let entries: Vec<(Item, Option<Action>)> = vec![
        (Item::sep("Главное"), None),
        (Item::new("➤", "Опубликовать проект на GitHub", "папка → .gitignore → коммит → push"), Some(|a| project::publish_wizard(a, true))),
        (Item::new("▣", "Выбрать папку проекта", "проводник, путь или недавние"), Some(|a| { project::select_folder(a); })),
        (Item::new("◉", "Статус проекта", "изменения, ветка, синхронизация, diff"), Some(project::status_screen)),
        (Item::new("✍", "Коммит", "выбор файлов + сообщение + проверка секретов"), Some(project::commit_screen)),
        (Item::new("⇅", "Синхронизация", "push / pull / fetch"), Some(project::sync_screen)),
        (Item::new("⊘", ".gitignore", "создать, дополнить, выбрать файлы"), Some(|a| {
            if let Some(d) = project::need_project(a) {
                gitignore::wizard(&d);
                ui::pause();
            }
        })),
        (Item::sep("Инструменты"), None),
        (Item::new("⎇", "Ветки", "создать, переключить, слить, удалить"), Some(tools::branches)),
        (Item::new("☰", "История коммитов", "граф веток"), Some(tools::history)),
        (Item::new("↶", "Исправить ошибки", "отмена коммита, откат файлов, reflog"), Some(tools::undo)),
        (Item::new("◫", "Stash", "временно отложить изменения"), Some(tools::stash)),
        (Item::new("★", "Релиз / версия", "тег + релиз на GitHub с заметками"), Some(tools::release)),
        (Item::new("⛨", "Проверка безопасности", "секреты, ключи, файлы > 100 МБ"), Some(project::scan_screen)),
        (Item::new("▤", "README и LICENSE", "сгенерировать по шаблону"), Some(tools::docs)),
        (Item::new("▥", "Статистика", "языки, авторы, активность"), Some(tools::stats)),
        (Item::sep("GitHub"), None),
        (Item::new("⤓", "Клонировать репозиторий", "из списка твоих репозиториев"), Some(tools::clone_repo)),
        (Item::new("↗", "Открыть на GitHub", "репозиторий, PR, Issues, Actions"), Some(tools::browser)),
        (Item::new("☺", "Аккаунт", "вход по почте/паролю или токену"), Some(auth::menu)),
        (Item::new("⚙", "Настройки", "анимации, видимость, ветка"), Some(tools::settings)),
        (Item::new("⏻", "Выход", ""), None),
    ];
    let items: Vec<Item> = entries
        .iter()
        .map(|(i, _)| Item { label: i.label.clone(), hint: i.hint.clone(), icon: i.icon.clone(), sep: i.sep })
        .collect();
    let exit_idx = entries.len() - 1;
    let mut last = 1usize;
    loop {
        refresh_status(app);
        ui::header("");
        match ui::menu(&items, last) {
            None => {
                ui::blank();
                if ui::confirm("Выйти из GitHelper?", true) {
                    return;
                }
            }
            Some(i) if i == exit_idx => return,
            Some(i) => {
                last = i;
                if let Some(f) = entries[i].1 {
                    f(app);
                }
            }
        }
    }
}

fn goodbye() {
    ui::clear();
    let (w, h) = ui::size();
    let msg = "До встречи! Удачных коммитов ✦";
    let x = w.saturating_sub(msg.chars().count() as u16) / 2;
    let y = h / 2;
    if ui::anim_on() {
        for t in 0..=14 {
            let f = t as f32 / 14.0;
            ui::out(&format!("{}{}{}", ui::goto(x, y), ui::bg(ui::BG), ui::gradient_text(msg, ui::lerp(ui::BG, WHITE, f), ui::lerp(ui::BG, CYAN, f))));
            ui::frame(30);
        }
        ui::frame(500);
        for t in (0..=10).rev() {
            let f = t as f32 / 10.0;
            ui::out(&format!("{}{}{}", ui::goto(x, y), ui::bg(ui::BG), ui::gradient_text(msg, ui::lerp(ui::BG, WHITE, f), ui::lerp(ui::BG, CYAN, f))));
            ui::frame(25);
        }
    }
}
