# Changelog

[English](CHANGELOG.md) · [简体中文](CHANGELOG.zh-CN.md)

## 0.2.0 — Preview · 2026-11-14

### Added

- Restored folder tabs, six light/dark skins, reduced motion, and a resource panel.
- A local plugin center with capability review, installation, enable/disable, updates, removal, and context-menu integration.
- Four plugins: checksums, file inventory, text inspection, and directory space.
- A QuickJS SDK, example plugin, and `.fmp` packaging script.
- GitHub/About integration, shortcut help, diagnostics, bilingual project documentation, and Linux CI/templates.

### Improved

- Separate thumbnail/detail requests, bounded text rendering and cache weight, and cancellation of obsolete queued previews.
- Bounded completed-job results and lightweight status polling.
- Large-image concurrency and allocation limits, WebKit cache policy, idle Shelf release, and converter process-group cleanup.
- Rename to FileManager while preserving existing catalogs, settings, sessions, and undo data.
- Keep grid filenames and metadata readable without vertical clipping.

### Validation scope

68 local automated tests and final deb/AppImage startup checks on Ubuntu 24.04 KDE/X11. Wayland and wider distribution coverage are tracked in the roadmap.

## 0.1 — Initial local prototype

Introduced the file browser, safe operations/undo, previews, Markdown, workspaces, bookmarks, cleanup, package inventory, AI/MCP tools, Shelf, and KDE integration.
