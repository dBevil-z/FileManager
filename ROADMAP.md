# Roadmap

[English](ROADMAP.md) · [简体中文](ROADMAP.zh-CN.md)

FileManager focuses on Linux desktop workflows. Items below distinguish implemented features from future compatibility work. An AppImage alone is not evidence of support for every Linux distribution.

## 0.2 — Foundations available now

- [x] Tauri/Rust desktop application with persistent file operations and recovery.
- [x] Icons, list, columns, dual panes, and restored folder tabs.
- [x] Bounded thumbnail/detail caches, image concurrency, task results, and idle resource lifetimes.
- [x] Six light/dark skins, custom accents, folder styles, and reduced motion.
- [x] Plugin API v1: permission review, local installation, updates, enable/disable, removal, and context-menu integration.
- [x] Checksums, file inventory, text inspection, and directory-space plugins.
- [x] [Remote storage](docs/remote-storage.md): manage connections, browse folders, and transfer files over S3, OSS, SFTP, and FTP/FTPS.
- [x] Remote cursor pagination, batch file/directory transfers, and directory sync previews (0.2.1).
- [x] About, GitHub links, changelog, shortcuts, diagnostics, and bilingual project documentation.
- [x] Local core/frontend/native tests and deb/AppImage smoke checks.
- [ ] Broader long-running workload and desktop validation.

## 0.3 — Linux compatibility and distribution

- [ ] Establish and document the minimum glibc/WebKitGTK build baseline.
- [ ] Expand Ubuntu/Debian package and AppImage installation checks.
- [ ] Build and verify RPM packages on Fedora/openSUSE, including dependencies, SELinux, and portals.
- [ ] Provide a PKGBUILD and validate Arch's rolling-library environment.
- [ ] Validate GNOME/KDE Wayland on Ubuntu 26.04 KVM: file drag-and-drop, clipboard, portals, global shortcuts, Shelf, scaling, and multiple monitors.
- [x] Prepare GitHub CI, issue templates, and pull-request guidance.
- [ ] Expand CI into installation/desktop/resource regressions across distributions.
- [ ] Select the project license and complete third-party distribution notices.
- [ ] Publish versioned release notes, checksums, installation instructions, and recoverable updates.
- [ ] Evaluate ARM64 and Flatpak after the x86_64/glibc baseline is stable.

## 0.4 — Plugin and theme ecosystem

- [ ] Signed online catalog with categories, search, compatibility ranges, source links, release notes, and update notices.
- [ ] Publisher identity, permission-change review, revocation, and rollback.
- [ ] SDK type declarations, packaging/validation CLI, fixtures, and compatibility policy.
- [ ] Importable theme packs limited to supported visual tokens, with previews and export.
- [ ] More declarative inputs/results and individually reviewed host capabilities.

The current SDK intentionally exposes scoped file operations rather than arbitrary commands, unrestricted filesystem access, or resident background processes.

## 0.5 — More everyday workflows

Prefer plugins where the capability model fits:

- [ ] Archive workflows: ZIP/TAR/7z, safe path/link handling, conflict review, jobs, cancellation, and undo.
- [ ] Directory comparison and synchronization with a change preview and explicit direction.
- [ ] Saved searches and smart folders based on type, size, time, and tags, refreshed on demand.
- [ ] Batch image conversion, PDF tools, filename cleanup, and checksum-list verification.
- [ ] Large-directory pagination and repeatable performance budgets.
- [ ] Complete English coverage for new tools and expand translations.
- [ ] Keyboard, accessibility, high-contrast, and display-scaling acceptance checks.

## Platform boundaries

Linux is the only current implementation target. Core operations, storage, plugin manifests, and interface logic retain clear boundaries from OS-specific Trash, permissions, portals, clipboard, desktop integration, and packaging. Windows and macOS are possible future adaptations, not current releases or support claims.
