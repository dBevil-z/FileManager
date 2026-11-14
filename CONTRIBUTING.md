# Contributing

[English](CONTRIBUTING.md) · [简体中文](CONTRIBUTING.zh-CN.md)

Thanks for helping improve FileManager. Start with the [README](README.md), [roadmap](ROADMAP.md), and [plugin SDK](docs/plugins.md). Use [Issues](https://github.com/dBevil-z/FileManager/issues) for reproducible bugs or concrete workflow proposals.

## Development

Install the Linux dependencies listed in the README, Rust 1.91 or newer, Node.js 24, and pnpm 10:

```bash
pnpm install --frozen-lockfile
pnpm desktop
```

## Checks

```bash
pnpm check
pnpm test
pnpm build
cargo fmt --all --check
cargo test -p filemanager-core
# Isolate native clipboard tests from the active desktop:
GTK_USE_PORTAL=0 NO_AT_BRIDGE=1 xvfb-run -a dbus-run-session -- cargo test -p filemanager
pnpm tauri build
```

Native clipboard tests use a separate selection rather than overwriting the user's clipboard. Filesystem tests use temporary data and Trash directories. `FILEMANAGER_TEST_HOME` and `FILEMANAGER_DATA_DIR` isolate development scenarios; do not use real documents as destructive test fixtures.

The Linux workflow runs checks on Ubuntu 24.04. Stable tags trigger a separate signed Release workflow after validation. Public assets are immutable. See the [release guide](docs/releases.md); one CI environment does not guarantee other desktop environments.

## Architecture

- `crates/filemanager-core`: filesystem operations, jobs, SQLite, undo, previews, and plugin execution.
- `src-tauri`: GTK/WebKitGTK, clipboard, tray, global shortcuts, and KDE/D-Bus integration.
- `src`: React/TypeScript interface and shared theme tokens.
- `plugins`: independent JavaScript tools and a minimal template.

Consider a plugin for a new focused tool before expanding the application shell. SDK changes should update the API version/compatibility rules, template, documentation, and capability-boundary tests together.

## Pull requests

Describe the trigger, previous behavior, resulting behavior, and relevant verification. For desktop changes, include the distribution, desktop, and X11/Wayland session. For interface changes, include actual light/dark screenshots and keyboard/focus checks.

Preserve conflict checks, cancellation, and recovery in file operations. For preview or plugin work, check cache bytes, background-process lifetime, and idle release. Keep generated packages, logs, local databases, credentials, and private documents out of commits.

English and Simplified Chinese contributions are welcome. Keep the paired public documentation synchronized when changing behavior or setup steps.

## License status

The project owner has not selected a source license yet. Do not add a license claim or replace third-party licensing without an explicit project decision.
