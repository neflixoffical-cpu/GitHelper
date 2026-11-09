//! Движок интерфейса: цвета (тема "color 9"), анимации и интерактивные виджеты.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, terminal};
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ───────────────────────── Цвета ─────────────────────────

#[derive(Clone, Copy)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// "color 9" из cmd — светло-синий на чёрном. Это основной цвет всего интерфейса.
pub const PRIMARY: Rgb = Rgb(59, 120, 255);
pub const DEEP: Rgb = Rgb(0, 55, 218);
pub const CYAN: Rgb = Rgb(97, 214, 255);
pub const ICE: Rgb = Rgb(190, 220, 255);
pub const WHITE: Rgb = Rgb(240, 246, 255);
pub const DIM: Rgb = Rgb(52, 82, 150);
pub const FAINT: Rgb = Rgb(28, 44, 86);
pub const OK: Rgb = Rgb(80, 230, 170);
pub const WARN: Rgb = Rgb(255, 210, 110);
pub const ERR: Rgb = Rgb(255, 95, 110);
pub const BG: Rgb = Rgb(0, 0, 0);
pub const SEL_BG: Rgb = Rgb(14, 34, 92);

pub fn fg(c: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
}
pub fn bg(c: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m", c.0, c.1, c.2)
}
pub const BOLD: &str = "\x1b[1m";
pub const NOBOLD: &str = "\x1b[22m";

pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// Градиент темы: глубокий синий → color 9 → голубой.
pub fn grad(t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        lerp(DEEP, PRIMARY, t * 2.0)
    } else {
        lerp(PRIMARY, CYAN, (t - 0.5) * 2.0)
    }
}

pub fn gradient_text(s: &str, from: Rgb, to: Rgb) -> String {
    let n = s.chars().count().max(2) - 1;
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        out.push_str(&fg(lerp(from, to, i as f32 / n as f32)));
        out.push(ch);
    }
    out.push_str(&fg(PRIMARY));
    out
}

pub fn c(col: Rgb, s: &str) -> String {
    format!("{}{}{}", fg(col), s, fg(PRIMARY))
}

/// Длина строки без ANSI-последовательностей.
pub fn vis_len(s: &str) -> usize {
    let mut n = 0;
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            if ch.is_ascii_alphabetic() {
                esc = false;
            }
        } else if ch == '\x1b' {
            esc = true;
        } else {
            n += 1;
        }
    }
    n
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

// ───────────────────────── Анимации: скорость ─────────────────────────

/// 0 — выкл, 1 — быстро, 2 — плавно (по умолчанию)
static ANIM: AtomicU8 = AtomicU8::new(2);
static RAW: AtomicBool = AtomicBool::new(false);

pub fn set_anim(level: u8) {
    ANIM.store(level.min(2), Ordering::Relaxed);
}
pub fn anim_on() -> bool {
    ANIM.load(Ordering::Relaxed) > 0
}
pub fn frame(ms: u64) {
    match ANIM.load(Ordering::Relaxed) {
        0 => {}
        1 => std::thread::sleep(Duration::from_millis(ms / 2)),
        _ => std::thread::sleep(Duration::from_millis(ms)),
    }
}

// ───────────────────────── Терминал ─────────────────────────

pub fn size() -> (u16, u16) {
    terminal::size().unwrap_or((100, 30))
}
pub fn width() -> usize {
    size().0 as usize
}

pub fn out(s: &str) {
    let mut o = stdout();
    let _ = o.write_all(s.as_bytes());
    let _ = o.flush();
}

/// Печать строки (в raw-режиме нужен \r\n).
pub fn line(s: &str) {
    out(&format!("{}{}{}\x1b[K\r\n", bg(BG), fg(PRIMARY), s));
}
pub fn blank() {
    line("");
}
pub fn goto(x: u16, y: u16) -> String {
    format!("\x1b[{};{}H", y + 1, x + 1)
}

pub fn init() {
    let _ = crossterm::ansi_support::supports_ansi();
    let _ = crossterm::execute!(stdout(), terminal::SetTitle("GitHelper"));
    enter_raw();
    out(&format!("{}{}\x1b[?25l\x1b[2J\x1b[H", bg(BG), fg(PRIMARY)));
}

pub fn shutdown() {
    leave_raw();
    out("\x1b[0m\x1b[?25h\x1b[2J\x1b[H");
}

pub fn enter_raw() {
    if !RAW.swap(true, Ordering::SeqCst) {
        let _ = terminal::enable_raw_mode();
    }
    out("\x1b[?25l");
}
pub fn leave_raw() {
    if RAW.swap(false, Ordering::SeqCst) {
        let _ = terminal::disable_raw_mode();
    }
    out("\x1b[?25h");
}

pub fn clear() {
    out(&format!("{}{}\x1b[2J\x1b[3J\x1b[H", bg(BG), fg(PRIMARY)));
}

/// Выполнить что-то в обычном (не raw) режиме — например, интерактивный установщик.
pub fn cooked<T>(f: impl FnOnce() -> T) -> T {
    leave_raw();
    out("\x1b[0m");
    let r = f();
    enter_raw();
    out(&format!("{}{}", bg(BG), fg(PRIMARY)));
    r
}

pub fn read_key() -> KeyEvent {
    loop {
        if let Ok(Event::Key(k)) = event::read() {
            if k.kind == KeyEventKind::Press {
                return k;
            }
        }
    }
}

fn drain_keys() {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }
}

// ───────────────────────── Шапка и статус ─────────────────────────

static STATUS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

pub fn set_status(items: Vec<(String, String)>) {
    *STATUS.lock().unwrap() = items;
}

const MINI_LOGO: &str = "◆ GitHelper";

/// Шапка каждого экрана: логотип, путь (хлебные крошки), строка статуса.
pub fn header(crumb: &str) {
    clear();
    let w = width();
    blank();
    let logo = gradient_text(MINI_LOGO, CYAN, PRIMARY);
    let crumb_s = if crumb.is_empty() {
        String::new()
    } else {
        format!("{}  ›  {}{}{}", fg(DIM), fg(ICE), crumb, fg(PRIMARY))
    };
    line(&format!("  {}{}{}{}", BOLD, logo, NOBOLD, crumb_s));
    // строка статуса
    let st = STATUS.lock().unwrap().clone();
    let mut s = String::from("  ");
    for (i, (k, v)) in st.iter().enumerate() {
        if i > 0 {
            s.push_str(&c(FAINT, "  │  "));
        }
        s.push_str(&format!("{}{} {}{}", fg(DIM), k, fg(ICE), v));
    }
    s.push_str(&fg(PRIMARY));
    line(&s);
    // анимированная линия-разделитель
    let len = w.saturating_sub(4).min(110);
    if anim_on() {
        let steps = 12;
        for st in 1..=steps {
            let n = len * st / steps;
            let mut l = String::from("  ");
            for i in 0..n {
                l.push_str(&fg(grad(i as f32 / len as f32)));
                l.push('━');
            }
            out(&format!("\r{}{}\x1b[K", bg(BG), l));
            frame(8);
        }
        out(&format!("{}\r\n", fg(PRIMARY)));
    } else {
        let mut l = String::from("  ");
        for i in 0..len {
            l.push_str(&fg(grad(i as f32 / len as f32)));
            l.push('━');
        }
        line(&l);
    }
    blank();
}

// ───────────────────────── Сообщения ─────────────────────────

pub fn info(s: &str) {
    line(&format!("  {} {}", c(CYAN, "●"), s));
}
pub fn ok(s: &str) {
    line(&format!("  {} {}", c(OK, "✔"), c(OK, s)));
}
pub fn warn(s: &str) {
    line(&format!("  {} {}", c(WARN, "▲"), c(WARN, s)));
}
pub fn err(s: &str) {
    line(&format!("  {} {}", c(ERR, "✖"), c(ERR, s)));
}
pub fn dim(s: &str) {
    line(&format!("  {}", c(DIM, s)));
}
pub fn title(s: &str) {
    line(&format!("  {}{}{}", BOLD, gradient_text(s, WHITE, CYAN), NOBOLD));
}
pub fn kv(k: &str, v: &str) {
    line(&format!("  {}{:<18}{} {}", fg(DIM), k, fg(ICE), v));
}

/// Текст печатается как на машинке.
pub fn typewriter(s: &str, col: Rgb) {
    if !anim_on() {
        line(&format!("  {}", c(col, s)));
        return;
    }
    out(&format!("{}  {}", bg(BG), fg(col)));
    for ch in s.chars() {
        out(&ch.to_string());
        frame(if ch == ' ' { 4 } else { 11 });
    }
    out(&format!("{}\x1b[K\r\n", fg(PRIMARY)));
}

/// Вспышка-подтверждение: галочка «проявляется», затем текст.
pub fn success(s: &str) {
    if anim_on() {
        let frames = ["·", "∘", "○", "◎", "◉", "✔"];
        for (i, f) in frames.iter().enumerate() {
            let col = lerp(DEEP, OK, i as f32 / (frames.len() - 1) as f32);
            out(&format!("\r{}  {}{}\x1b[K", bg(BG), fg(col), f));
            frame(45);
        }
        let n = s.chars().count();
        for k in 1..=n {
            let part: String = s.chars().take(k).collect();
            let head = lerp(WHITE, OK, 0.2);
            out(&format!("\r  {}✔ {}{}", fg(OK), fg(head), part));
            frame(6);
        }
        out(&format!("\r  {}✔ {}{}\x1b[K\r\n", fg(OK), BOLD, s));
        out(NOBOLD);
    } else {
        ok(s);
    }
}

pub fn pause() {
    blank();
    line(&format!("  {}", c(DIM, "Нажми любую клавишу, чтобы продолжить…")));
    drain_keys();
    read_key();
}

// ───────────────────────── Спиннер ─────────────────────────

const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Выполняет `f` в фоне, пока на экране крутится плавный градиентный спиннер.
pub fn spin<T: Send>(msg: &str, f: impl FnOnce() -> T + Send) -> T {
    let start = Instant::now();
    let result = std::thread::scope(|s| {
        let h = s.spawn(f);
        let mut i = 0usize;
        while !h.is_finished() {
            let t = (start.elapsed().as_millis() as f32 / 600.0).sin() * 0.5 + 0.5;
            let col = lerp(PRIMARY, CYAN, t);
            let el = start.elapsed().as_secs_f32();
            let time = if el > 2.0 { c(FAINT, &format!("  {:.0}с", el)) } else { String::new() };
            out(&format!(
                "\r{}  {}{} {}{}{}\x1b[K",
                bg(BG),
                fg(col),
                SPIN[i % SPIN.len()],
                gradient_text(msg, ICE, PRIMARY),
                c(DIM, "…"),
                time
            ));
            i += 1;
            std::thread::sleep(Duration::from_millis(70));
        }
        h.join().expect("фоновая задача упала")
    });
    out("\r\x1b[K");
    result
}

/// Плавная полоса прогресса (суб-символьная точность).
pub fn progress_bar(label: &str, frac: f32, w: usize) -> String {
    const PARTS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    let frac = frac.clamp(0.0, 1.0);
    let total = frac * w as f32;
    let full = total.floor() as usize;
    let rem = ((total - full as f32) * 8.0).floor() as usize;
    let mut s = String::new();
    for i in 0..w {
        let col = grad(i as f32 / w as f32);
        if i < full {
            s.push_str(&fg(col));
            s.push('█');
        } else if i == full && rem > 0 {
            s.push_str(&fg(col));
            s.push(PARTS[rem - 1]);
        } else {
            s.push_str(&fg(FAINT));
            s.push('─');
        }
    }
    format!("  {:<26}{} {}{:>4.0}%{}", label, s, fg(ICE), frac * 100.0, fg(PRIMARY))
}

// ───────────────────────── Меню ─────────────────────────

pub struct Item {
    pub label: String,
    pub hint: String,
    pub icon: String,
    pub sep: bool,
}

impl Item {
    pub fn new(icon: &str, label: &str, hint: &str) -> Item {
        Item { label: label.into(), hint: hint.into(), icon: icon.into(), sep: false }
    }
    pub fn sep(label: &str) -> Item {
        Item { label: label.into(), hint: String::new(), icon: String::new(), sep: true }
    }
}

fn render_item(it: &Item, selected: bool, num: Option<usize>, w: usize) -> String {
    if it.sep {
        let lbl = format!(" {} ", it.label);
        let rest = 34usize.saturating_sub(lbl.chars().count());
        return format!("   {}{}{}{}", fg(DIM), lbl, fg(FAINT), "─".repeat(rest));
    }
    let key = match num {
        Some(n) if n < 9 => format!("{}", n + 1),
        Some(n) if n < 9 + 26 => ((b'a' + (n - 9) as u8) as char).to_string(),
        _ => " ".into(),
    };
    let label = truncate(&it.label, 40);
    let hint_space = w.saturating_sub(58);
    let hint = truncate(&it.hint, hint_space);
    if selected {
        format!(
            "  {}{}▌{}{} {}  {}{:<40}{}{} {}{}{}",
            bg(SEL_BG),
            fg(CYAN),
            fg(WHITE),
            BOLD,
            it.icon,
            "",
            label,
            NOBOLD,
            fg(ICE),
            hint,
            bg(BG),
            fg(PRIMARY)
        )
    } else {
        format!(
            "  {} {}{} {}{}  {:<40} {}{}",
            fg(FAINT),
            fg(DIM),
            key,
            fg(PRIMARY),
            it.icon,
            label,
            fg(FAINT),
            hint
        ) + &fg(PRIMARY)
    }
}

/// Вертикальное меню со стрелками, горячими клавишами и анимацией появления.
/// Возвращает индекс выбранного пункта (с учётом разделителей) или None при Esc.
pub fn menu(items: &[Item], start: usize) -> Option<usize> {
    let selectable: Vec<usize> = (0..items.len()).filter(|&i| !items[i].sep).collect();
    if selectable.is_empty() {
        return None;
    }
    let mut cur = selectable.iter().position(|&i| i == start).unwrap_or(0);
    let (_, top) = cursor::position().unwrap_or((0, 5));
    let (w, h) = size();
    let w = w as usize;
    let avail = (h as usize).saturating_sub(top as usize + 3).max(5);
    let mut scroll = 0usize;

    let number_of = |idx: usize| selectable.iter().position(|&i| i == idx);

    let mut first = true;
    loop {
        let sel_idx = selectable[cur];
        if sel_idx < scroll {
            scroll = sel_idx;
        }
        if sel_idx >= scroll + avail {
            scroll = sel_idx + 1 - avail;
        }
        let visible = items.len().min(avail);
        let mut buf = String::new();
        for row in 0..visible {
            let idx = scroll + row;
            if idx >= items.len() {
                break;
            }
            let s = render_item(&items[idx], idx == sel_idx, number_of(idx), w);
            buf.push_str(&format!("{}{}{}{}\x1b[K", goto(0, top + row as u16), bg(BG), fg(PRIMARY), s));
            if first && anim_on() {
                // каскадное появление пунктов
                out(&buf);
                buf.clear();
                frame(14);
            }
        }
        let foot = format!(
            "{}{}  {}↑↓{} выбор   {}Enter{} открыть   {}1-9 a-z{} быстрый выбор   {}Esc{} назад{}\x1b[K",
            goto(0, top + visible as u16 + 1),
            fg(FAINT),
            fg(DIM),
            fg(FAINT),
            fg(DIM),
            fg(FAINT),
            fg(DIM),
            fg(FAINT),
            fg(DIM),
            fg(FAINT),
            fg(PRIMARY)
        );
        buf.push_str(&foot);
        out(&buf);
        first = false;

        let k = read_key();
        match k.code {
            KeyCode::Up => cur = if cur == 0 { selectable.len() - 1 } else { cur - 1 },
            KeyCode::Down | KeyCode::Tab => cur = (cur + 1) % selectable.len(),
            KeyCode::Home => cur = 0,
            KeyCode::End => cur = selectable.len() - 1,
            KeyCode::PageUp => cur = cur.saturating_sub(8),
            KeyCode::PageDown => cur = (cur + 8).min(selectable.len() - 1),
            KeyCode::Enter | KeyCode::Right => {
                flash_row(&items[selectable[cur]], top + (selectable[cur] - scroll) as u16, w);
                return Some(selectable[cur]);
            }
            KeyCode::Esc | KeyCode::Left | KeyCode::Backspace => return None,
            KeyCode::Char(ch) => {
                let n = if ('1'..='9').contains(&ch) {
                    Some(ch as usize - '1' as usize)
                } else if ch.is_ascii_lowercase() {
                    Some(9 + (ch as usize - 'a' as usize))
                } else {
                    None
                };
                if let Some(n) = n {
                    if n < selectable.len() {
                        cur = n;
                        flash_row(&items[selectable[cur]], top + (selectable[cur] - scroll) as u16, w);
                        return Some(selectable[cur]);
                    }
                }
                if ch == 'q' && k.modifiers.contains(KeyModifiers::CONTROL) {
                    return None;
                }
            }
            _ => {}
        }
    }
}

/// Короткая «вспышка» выбранного пункта.
fn flash_row(it: &Item, y: u16, w: usize) {
    if !anim_on() {
        return;
    }
    for t in [0.9f32, 0.6, 0.3, 0.0] {
        let b = lerp(SEL_BG, Rgb(60, 110, 230), t);
        let s = render_item(it, true, None, w).replace(&bg(SEL_BG), &bg(b));
        out(&format!("{}{}{}\x1b[K", goto(0, y), bg(BG), s));
        frame(28);
    }
}

/// Простой выбор из списка строк (с заголовком).
pub fn choose(question: &str, options: &[&str]) -> Option<usize> {
    line(&format!("  {} {}", c(CYAN, "?"), c(WHITE, question)));
    blank();
    let items: Vec<Item> = options.iter().map(|o| Item::new("›", o, "")).collect();
    menu(&items, 0)
}

// ───────────────────────── Ввод текста ─────────────────────────

/// Поле ввода с курсором, поддержкой кириллицы, значением по умолчанию и Esc для отмены.
pub fn input(prompt: &str, default: &str, masked: bool) -> Option<String> {
    let mut buf: Vec<char> = default.chars().collect();
    let mut pos = buf.len();
    out("\x1b[?25h");
    loop {
        let shown: String = if masked { "•".repeat(buf.len()) } else { buf.iter().collect() };
        let before: String = if masked { "•".repeat(pos) } else { buf[..pos].iter().collect() };
        let ph = if buf.is_empty() && !default.is_empty() { String::new() } else { String::new() };
        let pre = format!("  {} {} {}› ", c(CYAN, "?"), c(WHITE, prompt), fg(DIM));
        out(&format!(
            "\r{}{}{}{}{}\x1b[K\r{}{}{}",
            bg(BG),
            pre,
            fg(ICE),
            shown,
            ph,
            pre,
            fg(ICE),
            before
        ));
        let k = read_key();
        match k.code {
            KeyCode::Enter => {
                out(&format!("{}\r\n", fg(PRIMARY)));
                out("\x1b[?25l");
                return Some(buf.iter().collect::<String>().trim().to_string());
            }
            KeyCode::Esc => {
                out(&format!("{}\r\n", fg(PRIMARY)));
                out("\x1b[?25l");
                return None;
            }
            KeyCode::Backspace => {
                if pos > 0 {
                    pos -= 1;
                    buf.remove(pos);
                }
            }
            KeyCode::Delete => {
                if pos < buf.len() {
                    buf.remove(pos);
                }
            }
            KeyCode::Left => pos = pos.saturating_sub(1),
            KeyCode::Right => pos = (pos + 1).min(buf.len()),
            KeyCode::Home => pos = 0,
            KeyCode::End => pos = buf.len(),
            KeyCode::Char(ch) => {
                if k.modifiers.contains(KeyModifiers::CONTROL) {
                    if ch == 'u' {
                        buf.clear();
                        pos = 0;
                    }
                    continue;
                }
                buf.insert(pos, ch);
                pos += 1;
            }
            _ => {}
        }
    }
}

/// Ввод, который не может быть пустым.
pub fn input_req(prompt: &str, default: &str) -> Option<String> {
    loop {
        let v = input(prompt, default, false)?;
        if !v.is_empty() {
            return Some(v);
        }
        warn("Поле не может быть пустым");
    }
}

/// Да/Нет с переключателем стрелками.
pub fn confirm(q: &str, default: bool) -> bool {
    let mut yes = default;
    loop {
        let (y, n) = if yes {
            (format!("{}{}{} Да {}{}", bg(SEL_BG), fg(WHITE), BOLD, NOBOLD, bg(BG)), c(DIM, "  Нет  "))
        } else {
            (c(DIM, "  Да  "), format!("{}{}{} Нет {}{}", bg(SEL_BG), fg(WHITE), BOLD, NOBOLD, bg(BG)))
        };
        out(&format!("\r{}  {} {}   {} {}{}\x1b[K", bg(BG), c(CYAN, "?"), c(WHITE, q), y, n, fg(PRIMARY)));
        let k = read_key();
        match k.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => yes = !yes,
            KeyCode::Char('y') | KeyCode::Char('д') | KeyCode::Char('l') => {
                yes = true;
                break;
            }
            KeyCode::Char('n') | KeyCode::Char('н') | KeyCode::Char('т') => {
                yes = false;
                break;
            }
            KeyCode::Enter => break,
            KeyCode::Esc => {
                yes = false;
                break;
            }
            _ => {}
        }
    }
    out("\r\n");
    yes
}

// ───────────────────────── Чек-лист ─────────────────────────

pub struct Check {
    pub label: String,
    pub hint: String,
    pub on: bool,
    pub color: Rgb,
}

/// Список с галочками: ↑↓ навигация, Пробел — отметить, A — все, N — ничего, / — фильтр.
pub fn checklist(title: &str, items: &mut [Check]) -> bool {
    if items.is_empty() {
        return true;
    }
    line(&format!("  {} {}", c(CYAN, "?"), c(WHITE, title)));
    blank();
    let (_, top) = cursor::position().unwrap_or((0, 6));
    let (w, h) = size();
    let avail = (h as usize).saturating_sub(top as usize + 4).max(4);
    let mut cur = 0usize;
    let mut scroll = 0usize;
    let mut filter = String::new();
    let mut filtering = false;
    let mut first = true;
    loop {
        let view: Vec<usize> = (0..items.len())
            .filter(|&i| filter.is_empty() || items[i].label.to_lowercase().contains(&filter.to_lowercase()))
            .collect();
        if cur >= view.len() {
            cur = view.len().saturating_sub(1);
        }
        if cur < scroll {
            scroll = cur;
        }
        if cur >= scroll + avail {
            scroll = cur + 1 - avail;
        }
        let mut buf = String::new();
        for row in 0..avail {
            buf.push_str(&format!("{}{}\x1b[K", goto(0, top + row as u16), bg(BG)));
            let vi = scroll + row;
            if vi >= view.len() {
                continue;
            }
            let it = &items[view[vi]];
            let mark = if it.on { c(CYAN, "◉") } else { c(FAINT, "○") };
            let lbl = truncate(&it.label, (w as usize).saturating_sub(30).max(20));
            if vi == cur {
                buf.push_str(&format!(
                    "  {}{}▌ {} {}{}{}  {}{}{}",
                    bg(SEL_BG),
                    fg(CYAN),
                    mark.replace(&fg(PRIMARY), &fg(CYAN)),
                    fg(WHITE),
                    lbl,
                    fg(ICE),
                    it.hint,
                    bg(BG),
                    fg(PRIMARY)
                ));
            } else {
                buf.push_str(&format!("    {} {}{}{}  {}{}", mark, fg(it.color), lbl, fg(FAINT), it.hint, fg(PRIMARY)));
            }
            if first && anim_on() && row < 25 {
                out(&buf);
                buf.clear();
                frame(8);
            }
        }
        first = false;
        let sel = items.iter().filter(|i| i.on).count();
        let more = if view.len() > avail { format!("  {}/{} ", cur + 1, view.len()) } else { String::new() };
        let ftxt = if filtering || !filter.is_empty() {
            format!("   {}фильтр: {}{}{}", fg(DIM), fg(ICE), filter, if filtering { "▏" } else { "" })
        } else {
            String::new()
        };
        buf.push_str(&format!(
            "{}{}  {}выбрано {}{}{}{}{}\x1b[K",
            goto(0, top + avail as u16),
            bg(BG),
            fg(DIM),
            fg(CYAN),
            sel,
            fg(FAINT),
            more,
            ftxt
        ));
        buf.push_str(&format!(
            "{}{}  {}Пробел{} отметить  {}A{} все  {}N{} ничего  {}/{} поиск  {}Enter{} готово  {}Esc{} отмена{}\x1b[K",
            goto(0, top + avail as u16 + 1),
            bg(BG),
            fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(DIM), fg(FAINT), fg(DIM), fg(FAINT),
            fg(PRIMARY)
        ));
        out(&buf);

        let k = read_key();
        if filtering {
            match k.code {
                KeyCode::Enter | KeyCode::Esc => filtering = false,
                KeyCode::Backspace => {
                    filter.pop();
                }
                KeyCode::Char(ch) => {
                    filter.push(ch);
                    cur = 0;
                }
                _ => {}
            }
            continue;
        }
        match k.code {
            KeyCode::Up => cur = if cur == 0 { view.len().saturating_sub(1) } else { cur - 1 },
            KeyCode::Down => cur = if view.is_empty() { 0 } else { (cur + 1) % view.len() },
            KeyCode::PageUp => cur = cur.saturating_sub(avail),
            KeyCode::PageDown => cur = (cur + avail).min(view.len().saturating_sub(1)),
            KeyCode::Char(' ') => {
                if let Some(&i) = view.get(cur) {
                    items[i].on = !items[i].on;
                }
            }
            KeyCode::Char('a') | KeyCode::Char('ф') => view.iter().for_each(|&i| items[i].on = true),
            KeyCode::Char('n') | KeyCode::Char('т') => view.iter().for_each(|&i| items[i].on = false),
            KeyCode::Char('/') => {
                filtering = true;
            }
            KeyCode::Enter => {
                out(&format!("{}\r\n", goto(0, top + avail as u16 + 2)));
                return true;
            }
            KeyCode::Esc => {
                out(&format!("{}\r\n", goto(0, top + avail as u16 + 2)));
                return false;
            }
            _ => {}
        }
    }
}

// ───────────────────────── Просмотрщик ─────────────────────────

/// Прокручиваемый просмотр длинного текста (лог, diff, предпросмотр файла).
pub fn pager(title_s: &str, lines: &[String]) {
    let (_, h) = size();
    let mut top = 0usize;
    loop {
        header(title_s);
        let (_, y0) = cursor::position().unwrap_or((0, 5));
        let avail = (h as usize).saturating_sub(y0 as usize + 2).max(3);
        let mut buf = String::new();
        for i in 0..avail {
            let s = lines.get(top + i).map(|s| s.as_str()).unwrap_or("");
            buf.push_str(&format!("{}{}{}  {}\x1b[K", goto(0, y0 + i as u16), bg(BG), fg(PRIMARY), s));
        }
        let pct = if lines.len() <= avail { 100 } else { (top + avail).min(lines.len()) * 100 / lines.len() };
        buf.push_str(&format!(
            "{}{}  {}↑↓ PgUp PgDn{} прокрутка   {}Esc{} закрыть   {}{}%  ({} строк){}\x1b[K",
            goto(0, y0 + avail as u16),
            bg(BG),
            fg(DIM),
            fg(FAINT),
            fg(DIM),
            fg(FAINT),
            fg(CYAN),
            pct,
            lines.len(),
            fg(PRIMARY)
        ));
        out(&buf);
        loop {
            let max_top = lines.len().saturating_sub(avail);
            let k = read_key();
            let old = top;
            match k.code {
                KeyCode::Up => top = top.saturating_sub(1),
                KeyCode::Down => top = (top + 1).min(max_top),
                KeyCode::PageUp => top = top.saturating_sub(avail),
                KeyCode::PageDown | KeyCode::Char(' ') => top = (top + avail).min(max_top),
                KeyCode::Home => top = 0,
                KeyCode::End => top = max_top,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => return,
                _ => {}
            }
            if old != top {
                let mut buf = String::new();
                for i in 0..avail {
                    let s = lines.get(top + i).map(|s| s.as_str()).unwrap_or("");
                    buf.push_str(&format!("{}{}{}  {}\x1b[K", goto(0, y0 + i as u16), bg(BG), fg(PRIMARY), s));
                }
                let pct = (top + avail).min(lines.len()) * 100 / lines.len().max(1);
                buf.push_str(&format!(
                    "{}{}  {}↑↓ PgUp PgDn{} прокрутка   {}Esc{} закрыть   {}{}%  ({} строк){}\x1b[K",
                    goto(0, y0 + avail as u16),
                    bg(BG),
                    fg(DIM),
                    fg(FAINT),
                    fg(DIM),
                    fg(FAINT),
                    fg(CYAN),
                    pct,
                    lines.len(),
                    fg(PRIMARY)
                ));
                out(&buf);
            }
        }
    }
}

// ───────────────────────── Большой логотип и заставка ─────────────────────────

pub const LOGO: [&str; 6] = [
    " ██████╗ ██╗████████╗██╗  ██╗███████╗██╗     ██████╗ ███████╗██████╗ ",
    "██╔════╝ ██║╚══██╔══╝██║  ██║██╔════╝██║     ██╔══██╗██╔════╝██╔══██╗",
    "██║  ███╗██║   ██║   ███████║█████╗  ██║     ██████╔╝█████╗  ██████╔╝",
    "██║   ██║██║   ██║   ██╔══██║██╔══╝  ██║     ██╔═══╝ ██╔══╝  ██╔══██╗",
    "╚██████╔╝██║   ██║   ██║  ██║███████╗███████╗██║     ███████╗██║  ██║",
    " ╚═════╝ ╚═╝   ╚═╝   ╚═╝  ╚═╝╚══════╝╚══════╝╚═╝     ╚══════╝╚═╝  ╚═╝",
];

fn logo_frame(x0: u16, y0: u16, reveal: f32, shine: f32, fade: f32) -> String {
    let lw = LOGO[0].chars().count() as f32;
    let mut buf = String::new();
    for (row, l) in LOGO.iter().enumerate() {
        buf.push_str(&goto(x0, y0 + row as u16));
        buf.push_str(&bg(BG));
        for (col, ch) in l.chars().enumerate() {
            let x = col as f32;
            // диагональная развёртка
            let d = x + row as f32 * 2.0;
            if d > reveal {
                buf.push(' ');
                continue;
            }
            let base = grad(x / lw * 0.85 + row as f32 * 0.03);
            let base = if ch == '█' { base } else { lerp(base, DEEP, 0.55) };
            // свежепроявленные символы вспыхивают белым
            let fresh = (1.0 - (reveal - d) / 10.0).clamp(0.0, 1.0);
            let mut col_c = lerp(base, WHITE, fresh * 0.9);
            // блик
            let sd = (d - shine).abs();
            if sd < 6.0 {
                col_c = lerp(col_c, WHITE, (1.0 - sd / 6.0) * 0.75);
            }
            col_c = lerp(col_c, BG, fade);
            buf.push_str(&fg(col_c));
            buf.push(ch);
        }
    }
    buf
}

/// Заставка: диагональное проявление логотипа, блик, подзаголовок и загрузка.
pub fn splash(checks: &mut dyn FnMut(usize) -> (String, bool), steps: &[&str]) {
    clear();
    let (w, h) = size();
    let lw = LOGO[0].chars().count() as u16;
    let x0 = w.saturating_sub(lw) / 2;
    let y0 = (h / 2).saturating_sub(8);

    if anim_on() {
        let total = lw as f32 + 14.0 + 12.0;
        let mut r = 0.0;
        while r < total {
            out(&logo_frame(x0, y0, r, -100.0, 0.0));
            r += 1.6;
            frame(14);
        }
        // проход блика
        let mut s = -10.0;
        while s < lw as f32 + 20.0 {
            out(&logo_frame(x0, y0, 999.0, s, 0.0));
            s += 2.2;
            frame(12);
        }
    }
    out(&logo_frame(x0, y0, 999.0, -100.0, 0.0));

    // подзаголовок
    let sub = "Git и GitHub — без боли. Коммиты, .gitignore, публикация в один клик.";
    let sx = w.saturating_sub(sub.chars().count() as u16) / 2;
    out(&goto(sx, y0 + 7));
    if anim_on() {
        for (i, ch) in sub.chars().enumerate() {
            out(&format!("{}{}", fg(lerp(ICE, PRIMARY, i as f32 / 70.0)), ch));
            frame(7);
        }
    } else {
        out(&gradient_text(sub, ICE, PRIMARY));
    }
    let ver = format!("v{}  ·  color 9 edition", env!("CARGO_PKG_VERSION"));
    let vx = w.saturating_sub(ver.chars().count() as u16) / 2;
    out(&format!("{}{}{}", goto(vx, y0 + 8), fg(FAINT), ver));

    // загрузка с проверками
    let bw = 40usize;
    let by = y0 + 11;
    let bx = w.saturating_sub(bw as u16 + 34) / 2;
    let n = steps.len();
    for (i, st) in steps.iter().enumerate() {
        let from = i as f32 / n as f32;
        let to = (i + 1) as f32 / n as f32;
        let label = format!("{}{}", fg(ICE), st);
        // плавно доезжаем до середины шага, пока идёт проверка
        let start = Instant::now();
        let res = std::thread::scope(|sc| {
            let hnd = sc.spawn(|| ());
            let _ = hnd.join();
            let mut k = 0usize;
            let mut f = from;
            while start.elapsed() < Duration::from_millis(if anim_on() { 260 } else { 0 }) {
                f += (from + (to - from) * 0.7 - f) * 0.25;
                out(&format!(
                    "{}{}{}{} {}\x1b[K",
                    goto(bx, by),
                    bg(BG),
                    progress_bar(&strip(&label), f, bw),
                    fg(CYAN),
                    SPIN[k % SPIN.len()]
                ));
                k += 1;
                std::thread::sleep(Duration::from_millis(30));
            }
            checks(i)
        });
        // строка результата проверки
        let mark = if res.1 { c(OK, "✔") } else { c(WARN, "▲") };
        out(&format!("{}{}{} {}{}\x1b[K", goto(bx + 2, by + 2 + i as u16), bg(BG), mark, fg(DIM), res.0));
        let mut f = from + (to - from) * 0.7;
        for _ in 0..8 {
            f += (to - f) * 0.45;
            out(&format!("{}{}{}\x1b[K", goto(bx, by), bg(BG), progress_bar(st, f, bw)));
            frame(16);
        }
        out(&format!("{}{}{}\x1b[K", goto(bx, by), bg(BG), progress_bar(st, to, bw)));
    }
    out(&format!("{}{}{}\x1b[K", goto(bx, by), bg(BG), progress_bar("Готово", 1.0, bw)));
    frame(450);

    // затухание
    if anim_on() {
        for i in 1..=10 {
            let f = i as f32 / 10.0;
            out(&logo_frame(x0, y0, 999.0, -100.0, f));
            frame(22);
        }
    }
}

fn strip(s: &str) -> String {
    let mut out_s = String::new();
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            if ch.is_ascii_alphabetic() {
                esc = false;
            }
        } else if ch == '\x1b' {
            esc = true;
        } else {
            out_s.push(ch);
        }
    }
    out_s
}

/// Рамка вокруг блока строк.
pub fn boxed(lines_in: &[String], col: Rgb) {
    let wmax = lines_in.iter().map(|l| vis_len(l)).max().unwrap_or(0) + 2;
    line(&format!("  {}╭{}╮{}", fg(col), "─".repeat(wmax), fg(PRIMARY)));
    for l in lines_in {
        let pad = wmax - 1 - vis_len(l);
        line(&format!("  {}│{} {}{}{}│{}", fg(col), fg(PRIMARY), l, " ".repeat(pad), fg(col), fg(PRIMARY)));
    }
    line(&format!("  {}╰{}╯{}", fg(col), "─".repeat(wmax), fg(PRIMARY)));
}
