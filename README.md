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

## Discord Rich Presence (Linux)

Показывает текущий трек, исполнителя, обложку и время воспроизведения в настольном Discord.
На паузе таймер убирается, при остановке статус очищается. Обновление и повторное подключение
после запуска/перезапуска Discord выполняются раз в 5 секунд, независимо от аудиопотока.
Поддерживаются обычная установка и стандартные пути сокетов Flatpak/Snap (если сокет доступен).

Создайте приложение в https://discord.com/developers/applications и скопируйте **Application ID**:

```bash
DISCORD_APPLICATION_ID=ваш_числовой_id npm run tauri dev
# Для собранного приложения:
DISCORD_APPLICATION_ID=ваш_числовой_id ./src-tauri/target/release/sc-rust-client
```

В Discord разрешите отображение текущей активности. Название приложения в статусе берётся
из Discord Developer Portal. Токен бота не нужен. Без переменной `DISCORD_APPLICATION_ID`
интеграция выключена; после изменения переменной перезапустите клиент.

### Диагностика зависания окна WebKit (Linux)

Если окно перестаёт перерисовываться или WebKit падает, закройте предыдущий экземпляр
и попробуйте `npm run dev:compat`. Этот диагностический запуск отключает DMABUF renderer
и compositing WebKit только для данного процесса. Обычный запуск остаётся прежним.
Это проверка гипотезы о графическом выводе, а не подтверждённое исправление утечки памяти.
