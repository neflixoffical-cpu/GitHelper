# Как помочь проекту

Спасибо, что хочешь сделать GitHelper лучше! 💙

## Быстрый старт

```powershell
git clone https://github.com/<твой-ник>/GitHelper.git
cd GitHelper
cargo run
```

## Правила

- **Стиль кода:** перед коммитом запусти `cargo fmt` и `cargo clippy`
- **Коммиты:** формат [Conventional Commits](https://www.conventionalcommits.org/ru/), например `feat(gitignore): шаблон для Zig`. GitHelper умеет собирать такие сообщения сам 😉
- **Интерфейс:** все цвета берутся из констант в `src/ui.rs` (`PRIMARY`, `CYAN`, `DIM`…). Не добавляй новые цвета без необходимости: палитра `color 9` — часть стиля проекта
- **Анимации:** паузы делай только через `ui::frame(ms)`, чтобы работала настройка скорости и её выключение
- **Тексты:** интерфейс на русском, коротко и по-человечески

## Куда смотреть

| Хочу… | Файл |
|---|---|
| добавить шаблон `.gitignore` | `src/gitignore.rs` → `STACKS` |
| добавить шаблон секрета | `src/project.rs` → `patterns()` |
| добавить пункт меню | `src/main.rs` → `main_menu()` |
| новый виджет или анимацию | `src/ui.rs` |
| запрос к GitHub API | `src/github.rs` |

## Pull Request

1. Сделай форк и ветку `feature/что-то`
2. Проверь, что `cargo build --release` проходит без предупреждений
3. Опиши в PR, что изменилось; для интерфейса приложи скриншот или GIF
