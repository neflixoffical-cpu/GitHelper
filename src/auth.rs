//! Вход в GitHub: по токену, через браузер (почта + пароль), выход.

use crate::config;
use crate::git;
use crate::github;
use crate::ui::{self, c, Item, CYAN, DIM, ICE, OK, WARN, WHITE};
use crate::App;

pub fn menu(app: &mut App) {
    loop {
        ui::header("Аккаунт GitHub");
        match (&app.login, &app.token) {
            (Some(l), Some(_)) => {
                let name = app.cfg.name.clone().unwrap_or_default();
                ui::boxed(
                    &[
                        format!("{} {}", c(OK, "●"), c(WHITE, &format!("Вы вошли как @{}", l))),
                        c(DIM, &format!("{}  ·  способ входа: {}", name, app.cfg.auth_method)),
                    ],
                    CYAN,
                );
            }
            _ => ui::boxed(&[c(WARN, "○ Вы не вошли в GitHub"), c(DIM, "Вход нужен для публикации и работы с репозиториями")], ui::DIM),
        }
        ui::blank();
        let items = vec![
            Item::new("✉", "Войти по почте и паролю", "окно входа GitHub, поддерживает 2FA"),
            Item::new("⚿", "Войти по токену (PAT)", "classic или fine-grained"),
            Item::new("↗", "Создать токен на github.com", "откроет страницу с нужными правами"),
            Item::new("ⓘ", "Проверить вход и лимиты API", ""),
            Item::new("⏻", "Выйти из аккаунта", "удалить сохранённый токен"),
        ];
        match ui::menu(&items, 0) {
            Some(0) => login_browser(app),
            Some(1) => login_token(app),
            Some(2) => {
                let _ = open::that("https://github.com/settings/tokens/new?scopes=repo,workflow,delete_repo&description=GitHelper");
                ui::blank();
                ui::info("Открыл страницу создания токена: права repo и workflow уже отмечены.");
                ui::dim("Нажми «Generate token», скопируй его и выбери «Войти по токену».");
                ui::pause();
            }
            Some(3) => check(app),
            Some(4) => logout(app),
            _ => return,
        }
    }
}

fn finish_login(app: &mut App, token: String, method: &str) -> bool {
    let res = ui::spin("Проверяю учётные данные на GitHub", || github::user(&token));
    match res {
        Ok(u) => {
            if let Some(sc) = &u.scopes {
                if !sc.is_empty() && !sc.split(',').any(|s| s.trim() == "repo" || s.trim() == "public_repo") {
                    ui::warn("У токена нет права «repo» — публикация может не сработать.");
                }
            }
            if let Err(e) = config::save_token(&token) {
                ui::warn(&format!("Не смог сохранить токен в хранилище Windows: {}", e));
            }
            app.cfg.login = Some(u.login.clone());
            app.cfg.name = Some(if u.name.is_empty() { u.login.clone() } else { u.name.clone() });
            app.cfg.auth_method = method.to_string();
            app.cfg.save();
            app.login = Some(u.login.clone());
            app.token = Some(token.clone());
            ui::blank();
            ui::success(&format!("Добро пожаловать, {}!", app.cfg.name.clone().unwrap_or_default()));
            ui::kv("Публичных репо", &u.public_repos.to_string());
            ui::kv("Приватных репо", &u.private_repos.to_string());
            ui::kv("Хранение токена", "Диспетчер учётных данных Windows (зашифровано)");
            offer_identity(app, &u.email);
            true
        }
        Err(e) => {
            ui::err(&format!("Вход не удался: {}", e));
            false
        }
    }
}

/// Если git не знает имя/почту — предложить взять из GitHub.
pub fn offer_identity(app: &App, email: &Option<String>) {
    if git::config_get("user.name").is_some() && git::config_get("user.email").is_some() {
        return;
    }
    let (Some(login), Some(token)) = (&app.login, &app.token) else { return };
    ui::blank();
    ui::info("Git ещё не знает, от чьего имени делать коммиты.");
    let name = app.cfg.name.clone().unwrap_or(login.clone());
    let mail = email.clone().unwrap_or_else(|| github::noreply_email(token, login));
    ui::kv("Имя", &name);
    ui::kv("Почта", &mail);
    if ui::confirm("Настроить git с этими данными?", true) {
        let _ = git::run_global(&["config", "--global", "user.name", &name]);
        let _ = git::run_global(&["config", "--global", "user.email", &mail]);
        ui::success("Имя и почта для коммитов настроены");
    }
}

fn login_token(app: &mut App) {
    ui::header("Аккаунт › вход по токену");
    ui::dim("Токен выглядит как ghp_… или github_pat_…  Ввод скрыт.");
    ui::blank();
    let Some(t) = ui::input("Токен", "", true) else { return };
    if t.is_empty() {
        return;
    }
    finish_login(app, t, "токен");
    ui::pause();
}

fn login_browser(app: &mut App) {
    ui::header("Аккаунт › вход по почте и паролю");
    ui::boxed(
        &[
            c(WHITE, "Как это работает"),
            String::new(),
            c(ICE, "С 2021 года GitHub не принимает пароль напрямую в программах —"),
            c(ICE, "только через официальное окно входа. Поэтому GitHelper открывает его:"),
            c(ICE, "вводишь почту (или логин), пароль и код 2FA прямо на github.com."),
            String::new(),
            c(DIM, "Пароль не проходит через GitHelper — мы получаем только токен доступа."),
        ],
        CYAN,
    );
    ui::blank();
    if !git::has_gcm() {
        ui::warn("Не найден Git Credential Manager (идёт в комплекте с Git for Windows).");
        ui::dim("Переустанови Git с github.com/git-for-windows или войди по токену.");
        ui::pause();
        return;
    }
    if !ui::confirm("Открыть окно входа GitHub?", true) {
        return;
    }
    ui::blank();
    let res = ui::spin("Жду, пока ты войдёшь в открывшемся окне", git::credential_fill);
    match res {
        Ok((user, token)) => {
            if finish_login(app, token.clone(), "почта и пароль (браузер)") {
                git::credential_approve(&user, &token);
            } else {
                // устаревшие данные в хранилище — удаляем, чтобы в следующий раз спросило заново
                git::credential_reject(&user, &token);
                ui::info("Удалил устаревшие данные входа — попробуй ещё раз.");
            }
        }
        Err(e) => ui::err(&format!("Вход отменён или не удался: {}", if e.is_empty() { "окно закрыто" } else { &e })),
    }
    ui::pause();
}

fn check(app: &App) {
    ui::header("Аккаунт › проверка");
    let tok = app.token.clone();
    let res = ui::spin("Связываюсь с GitHub", || {
        let u = tok.as_deref().map(github::user);
        let rl = github::rate_limit(tok.as_deref());
        (u, rl)
    });
    match res.0 {
        Some(Ok(u)) => {
            ui::success(&format!("Токен действителен: @{}", u.login));
            ui::kv("Права токена", u.scopes.as_deref().filter(|s| !s.is_empty()).unwrap_or("fine-grained (по репозиториям)"));
        }
        Some(Err(e)) => ui::err(&e),
        None => ui::warn("Вход не выполнен"),
    }
    if let Some((rem, lim)) = res.1 {
        ui::blank();
        ui::line(&ui::progress_bar("Лимит API", rem as f32 / lim.max(1) as f32, 30));
        ui::dim(&format!("  осталось {} из {} запросов в час", rem, lim));
    }
    ui::pause();
}

fn logout(app: &mut App) {
    ui::blank();
    if !ui::confirm("Выйти и удалить сохранённый токен?", false) {
        return;
    }
    if app.cfg.auth_method.contains("браузер") {
        if let (Some(l), Some(t)) = (&app.login, &app.token) {
            git::credential_reject(l, t);
        }
    }
    config::delete_token();
    app.token = None;
    app.login = None;
    app.cfg.login = None;
    app.cfg.name = None;
    app.cfg.auth_method.clear();
    app.cfg.save();
    ui::success("Вы вышли из аккаунта");
    ui::pause();
}

/// Нужен вход — предлагает войти прямо сейчас.
pub fn require(app: &mut App) -> bool {
    if app.token.is_some() {
        return true;
    }
    ui::blank();
    ui::warn("Для этого нужно войти в GitHub.");
    if ui::confirm("Войти сейчас?", true) {
        menu(app);
    }
    app.token.is_some()
}
