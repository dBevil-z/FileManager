#!/usr/bin/env bash
# Install a local release for the current user without changing the default file manager.
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
filemanager_binary=${1:-"$project_dir/target/release/filemanager"}
if [[ ! -f "$filemanager_binary" || ! -x "$filemanager_binary" ]]; then
  printf 'Executable not found: %s\nBuild with pnpm tauri build --no-sign first.\n' "$filemanager_binary" >&2
  exit 1
fi
filemanager_data=${XDG_DATA_HOME:-"$HOME/.local/share"}
filemanager_bin_dir="$HOME/.local/bin"
filemanager_icon_dir="$filemanager_data/icons/hicolor/512x512/apps"
filemanager_install="$filemanager_data/filemanager-desktop/bin/filemanager"
filemanager_launcher="$filemanager_bin_dir/filemanager"
# Existing executables from other projects are not overwritten.
if [[ -e "$filemanager_launcher" || -L "$filemanager_launcher" ]]; then
  if [[ $(readlink -- "$filemanager_launcher" || true) != "$filemanager_install" ]]; then
    printf 'Preserving existing executable: %s\n' "$filemanager_launcher" >&2
    exit 1
  fi
fi
install -d -- "$filemanager_bin_dir" "$(dirname -- "$filemanager_install")" "$filemanager_data/applications" "$filemanager_icon_dir" "$filemanager_data/krunner/dbusplugins" "$filemanager_data/dbus-1/services"
install -m 0755 -- "$filemanager_binary" "$filemanager_install.new"
mv -f -- "$filemanager_install.new" "$filemanager_install"
ln -sfn -- "$filemanager_install" "$filemanager_launcher"
install -m 0644 -- "$project_dir/src-tauri/icons/icon.png" "$filemanager_icon_dir/io.filemanager.linux.png"
install -m 0644 -- "$project_dir/packaging/filemanager-runner.desktop" "$filemanager_data/krunner/dbusplugins/filemanager-runner.desktop"
python3 - "$project_dir" "$filemanager_data" "$filemanager_install" <<'PY'
import pathlib, sys
project, data, executable = map(pathlib.Path, sys.argv[1:])
# Exec lines follow the desktop entry escaping rules; no shell is involved.
quoted = '"' + ''.join('\\' + c if c in '\\"`$' else c for c in str(executable)) + '"'
quoted = quoted.replace('\\', '\\\\').replace('%', '%%')
text = (project / 'packaging/filemanager.desktop.hbs').read_text()
(data / 'applications/io.filemanager.linux.desktop').write_text(text.replace('{{exec}}', quoted).replace('{{icon}}','io.filemanager.linux'))
service = (project / 'packaging/io.filemanager.Runner.service').read_text()
(data / 'dbus-1/services/io.filemanager.Runner.service').write_text(service.replace('/usr/bin/filemanager', quoted.replace('%%','%')))
PY
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$filemanager_data/applications"
fi
printf 'Installed FileManager: %s\nDesktop entry: %s\n' "$filemanager_install" "$filemanager_data/applications/io.filemanager.linux.desktop"
# Remove only launchers created by the previous user installation. Its data and
# executable remain available for rollback; an unrelated executable is preserved.
python3 - "$filemanager_data" "$filemanager_bin_dir" <<'PY'
import pathlib,sys
base,bin_dir=map(pathlib.Path,sys.argv[1:])
old_exe=base/'fileos-desktop/bin/fileos'
link=bin_dir/'fileos'
if link.is_symlink() and link.readlink()==old_exe:
    link.unlink()
for path,marker in [
    (base/'applications/io.fileos.linux.desktop',str(old_exe)),
    (base/'dbus-1/services/io.fileos.Runner.service',str(old_exe)),
    (base/'krunner/dbusplugins/fileos-runner.desktop','X-Plasma-DBusRunner-Service=io.fileos.Runner')
]:
    if path.is_file() and marker in path.read_text():
        path.unlink()
PY
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$filemanager_data/applications"
fi
