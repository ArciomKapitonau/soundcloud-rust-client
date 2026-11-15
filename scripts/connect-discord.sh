#!/usr/bin/env bash
set -euo pipefail
id="${1:-}"
if [[ ! "$id" =~ ^[0-9]{17,20}$ ]]; then
    printf 'Usage: bash scripts/connect-discord.sh APPLICATION_ID\n' >&2
    exit 1
fi
app_dir="$HOME/.local/lib/sc-rust-client"
if [[ ! -x "$app_dir/sc-rust-client" ]]; then
    printf 'Install the app first: bash scripts/install-local.sh\n' >&2
    exit 1
fi
printf '%s\n' "$id" > "$app_dir/discord-application-id"
cat > "$app_dir/launch" <<'LAUNCH'
#!/usr/bin/env bash
app_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
if [[ -f "$app_dir/discord-application-id" ]]; then
    read -r DISCORD_APPLICATION_ID < "$app_dir/discord-application-id"
    export DISCORD_APPLICATION_ID
fi
exec "$app_dir/sc-rust-client" "$@"
LAUNCH
chmod 755 "$app_dir/launch"
printf 'Discord RPC configured. Quit SoundCloud using the tray menu and reopen it.\n'
