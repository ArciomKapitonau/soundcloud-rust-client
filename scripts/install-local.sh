#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
export PATH="$HOME/.cargo/bin:$PATH"
# Keep build parallelism modest on machines with limited available RAM.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
npm run tauri -- build --no-bundle --config src-tauri/tauri.compat.conf.json

app_dir="$HOME/.local/lib/sc-rust-client"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
install -d "$app_dir" "$data_dir/applications" "$data_dir/icons/hicolor/256x256/apps"
install -m 755 "${CARGO_TARGET_DIR:-src-tauri/target}/release/sc-rust-client" "$app_dir/sc-rust-client"
install -m 644 src-tauri/icons/icon.png "$data_dir/icons/hicolor/256x256/apps/sc-rust-client.png"
cat > "$app_dir/launch" <<'LAUNCH'
#!/usr/bin/env bash
app_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
if [[ -f "$app_dir/discord-application-id" ]]; then
    read -r DISCORD_APPLICATION_ID < "$app_dir/discord-application-id"
    export DISCORD_APPLICATION_ID
fi
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
exec "$(dirname -- "$0")/sc-rust-client" "$@"
LAUNCH
chmod 755 "$app_dir/launch"
python3 - "$app_dir/launch" "$data_dir/applications/sc-rust-client.desktop" <<'PY'
import sys
from pathlib import Path
# Desktop Exec has its own quoting rules, independent of the user's shell.
exe = sys.argv[1].replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%')
content = f'''[Desktop Entry]
Type=Application
Name=SoundCloud Rust
Comment=SoundCloud desktop player
Exec="{exe}"
Icon=sc-rust-client
Terminal=false
Categories=AudioVideo;Audio;Player;
StartupNotify=true
'''
Path(sys.argv[2]).write_text(content)
PY
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$data_dir/applications"
fi
printf 'Installed. Open SoundCloud Rust from the application menu.\n'
