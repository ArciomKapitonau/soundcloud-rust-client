# SoundCloud Rust Client

Десктопный клиент SoundCloud для Linux: Tauri v2 + Rust (воспроизведение на стороне Rust) + React/TypeScript/Tailwind.

## Запуск

```bash
fish_add_path ~/.cargo/bin      # bash/zsh: export PATH="$HOME/.cargo/bin:$PATH"
npm install
npm run tauri dev               # разработка
npm run tauri build             # .deb / .rpm / .AppImage в src-tauri/target/release/bundle/
```

Тесты бэкенда: `cd src-tauri && cargo test` (сетевой e2e-тест: `cargo test -- --ignored --nocapture`).

## Режимы входа

| Режим | API | Что нужно |
|---|---|---|
| OAuth 2.1 + PKCE | api.soundcloud.com | client_id + secret приложения с redirect URI `http://localhost:3000/callback` |
| Веб-токены | api-v2.soundcloud.com | `oauth_token` из cookie soundcloud.com (client_id необязателен) |
| Гость | api-v2.soundcloud.com | ничего: client_id извлекается из JS-бандлов soundcloud.com |

Сессия и настройки: `~/.config/sc-rust-client/{auth,settings}.json` (права 0600).

## Архитектура (`src-tauri/src`)

- `api.rs`: оба API, нормализация JSON, пагинация, лайки, парсинг client_id, выбор потока, вейвформы
- `auth.rs`: локальный callback-сервер, PKCE, обмен и обновление токенов
- `stream.rs`: потоковая загрузка (progressive и HLS) в растущий буфер с блокирующим reader
- `audio.rs`: декодирование symphonia → rodio; поток вывода; плеер с очередью, повтором и сменой устройства
- `mpris.rs`, `tray.rs`: медиаклавиши (MPRIS2) и системный трей (закрытие окна сворачивает в трей)
- `commands.rs`: Tauri-команды

SoundCloud сейчас отдаёт в основном HLS AAC (fMP4). Он демультиплексируется потоково, поэтому звук начинается примерно через 1 с,
а перемотка заново открывает декодер на нужном HLS-сегменте уже загруженных данных. Зашифрованные (DRM) потоки не поддерживаются.

## Темы

Настройки → «Внешний вид»: 6 пресетов (SoundCloud Dark, Cyberpunk / Phonk, Sigilkore / Y2K Trap, Hyprland / Rice,
Gachiakuta / Grime, Lolicore / Vapor Glitch) и редактор кастомного CSS. Всё применяется на лету и хранится в
`~/.config/sc-rust-client/theme.json`.

- Переменные определены в `src/index.css` (`--bg-primary`, `--bg-secondary`, `--accent`, `--text-primary`, `--text-secondary`,
  `--border-color`, `--radius`, `--backdrop-blur`, `--font-main`, а также `--accent-2`, `--wave-*` и др.). Утилиты Tailwind v4 связаны
  с ними через `@theme inline`, отдельного `tailwind.config.js` в v4 нет.
- Пресеты описаны в `src/lib/themes.ts`. Они подключаются через `<style id="theme-preset">`, пользовательский CSS — через
  `<style id="user-custom-theme">` после него, поэтому он может переопределить что угодно.
- Окно прозрачное: полупрозрачные темы (Hyprland, Lolicore) показывают обои с размытием Hyprland.

Горячие клавиши: Пробел — пауза, ←/→ — ±10 с, Shift+←/→ — предыдущий/следующий трек.
