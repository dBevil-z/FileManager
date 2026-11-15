# Changelog

[English](CHANGELOG.md) · [简体中文](CHANGELOG.zh-CN.md)

## 0.2.5 — Drop Shelf and file picker refinements · 2026-11-15

- Resize the Drop Shelf window and restore its size when it reopens after idle cleanup within the same session.
- Wrap Shelf items across rows, show up to two lines of each filename, and reveal the remove button on keyboard focus.
- Fit the file picker to shorter windows, with scrolling for the folder list, file list and selected items.

## 0.2.4 — Signed releases and updates · 2026-11-14

- Publish Linux deb, AppImage and user-install artifacts with SHA-256 checksums, a GitHub update feed and signatures bound to the release version.
- Add on-demand version checks, release notes, download progress, signature verification, explicit installation/restart and atomic user-install rollback copies.
- Guard updates against active background jobs; await browsing-session persistence before restarting.
- Consolidate official repository, guide, roadmap, issue, author and Release links; migrate the legacy default profile URL.
- Add tagged Release automation, public signing-key configuration and bilingual publishing/recovery instructions. Private signing material stays outside Git.
- Includes the 0.2.3 navigation-restoration changes below in the first packaged public release.

## 0.2.3 — Resume browsing · 2026-11-14

- Remember each menu’s directory/detail page, filters, view controls, selection, and scroll position across menu switches and restarts.
- Restore remote connections, Buckets, directories, and pages with fresh cursors after restart.
- Reattach to retained cleanup scan results during the current run without starting a new scan on each visit.
- Add controls to disable navigation memory and reset saved positions. Keep UI snapshots bounded; release inactive previews and expire remote metadata caches.

## 0.2.2 — MinIO accounts and bucket browsing · 2026-11-14

- Dedicated MinIO username/password sign-in, verified before saving.
- Optional MinIO/S3 bucket and region, automatic region discovery, and permission-scoped bucket listing.
- Bucket pagination, entry/return navigation, and direct bucket opening; cursors, transfers, and sync previews retain the selected bucket scope.

## 0.2.1 — Remote storage · 2026-11-14

- Built-in S3/MinIO, native OSS V4, SFTP, FTP, and explicit FTPS with connection management, read-only profiles, and optional system keyring credentials.
- Remote cursor pagination, cross-page selection, batch file and recursive directory transfers, including empty folders.
- Existing-file protection, individual failure reports, cancellation, and staging cleanup.
- Directional directory sync previews with path, size, and modification-time comparisons, filters, missing items, and conflicts. Equal sizes remain content-unverified.
- Upgrades preserve existing workspaces, settings, sessions, and operation history.

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
