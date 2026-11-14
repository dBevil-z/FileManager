# Validation record — 0.2

[English](verification.md) · [简体中文](verification.zh-CN.md)

Local validation date: **2026-11-14**. The tested environment is Linux x86_64, Ubuntu 24.04, KDE/X11. Ubuntu 26.04 Wayland/KVM and other distributions are planned validation targets, not current certifications. Repository CI status is available in [Actions](https://github.com/dBevil-z/FileManager/actions/workflows/linux.yml).

## Automated checks

| Scope        | Local result | Coverage                                                                                                                                                   |
| ------------ | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust core    | 53 passing   | File conflicts, cross-volume work/undo, legacy databases, cleanup, AI/MCP, preview budgets, converter groups, plugin boundaries, and retained job results. |
| Tauri/GTK    | 4 passing    | Argument normalization, file URIs, non-UTF-8 paths, deep-link routing, and a separate clipboard test selection.                                            |
| Frontend     | 11 passing   | Localization, tab restoration/navigation/closing, weighted caches, queued preview cancellation, theme contrast, and GitHub URLs.                           |
| Static/build | Passing      | TypeScript, Rustfmt, Prettier, whitespace checks, production frontend/native builds, and deb/AppImage packaging.                                           |

Total: **68 local automated tests**. The core groups are 2 job, 8 agent, 3 catalog, 18 file, 8 plugin, 5 preview, 1 process, and 8 tool tests. Upstream Markdown packages emit harmless empty-directive build warnings.

Plugin tests include traversal, unknown grants, changed installed content, out-of-scope reads, descendant symlink escapes, loop cancellation, JS heap exhaustion, missing host capabilities, and spreadsheet-formula escaping. Atomics/shared buffers/promises are not exposed in the runtime.

## Memory methodology

Samples read `/proc/.../smaps_rollup` for the application and its WebKit descendants. PSS apportions shared pages; summed RSS counts shared pages repeatedly. These are workload-specific observations, not universal memory ceilings.

A long-running 0.1 instance had reached approximately 1.4 GiB PSS / 1.5 GiB RSS in its main process, and about 2110 MiB PSS across its process tree. A separate stale headless converter from that preview pipeline was also found and stopped after ownership/profile verification.

A repeatable isolated Xvfb/D-Bus workload used 72 paths to a synthetic 4500×3000 JPEG (hard-linked), four directories, and 24 navigations, with separate state for each version:

| Stage                    | 0.1       | 0.2 candidate |
| ------------------------ | --------- | ------------- |
| Initial process-tree PSS | 224.9 MiB | 161.0 MiB     |
| After 24 navigations     | 334.4 MiB | 181.4 MiB     |
| After previews settled   | 317.9 MiB | 181.0 MiB     |

The hidden Shelf renderer exited after roughly 20–30 seconds, taking the process tree from four processes back to three. A subsequent short idle sample stabilized around 193 MiB PSS.

A final installed-build sample in a restored real directory/dual-pane session measured:

- Main process: 61.0 MiB PSS / 187.9 MiB RSS.
- Complete process tree: 197.2 MiB PSS / 522.4 MiB RSS, three processes.
- Five-second CPU sample: 0.6% of one logical core.
- A later sample: 197.8 MiB PSS / 523.0 MiB RSS and 0.4% CPU; the main-process values stayed unchanged.

The short workload does not reproduce or fully explain every retained allocation in the hours-old instance. Longer real usage remains part of validation.

Current budgets include 320×240 thumbnails, 2 KiB text thumbnails, 64 KiB text previews, an 8 MiB/64-entry frontend object cache, and up to eight retained completed-job results with 16 MiB serialized content/15-minute retention. Editing still supports complete text within its 5 MiB file limit. Normal job-status polling excludes full scan/report results. Plugin limits are documented in the SDK.

## Interface and actual files

Browser checks used the same Rust core with isolated Home/Trash/state:

- Reviewed and installed official tools; launched a checksum from a selected file's context menu and inspected its actual digest.
- Generated CSV/Markdown/JSON inventory reports, selected a test directory, saved a report, and undid that export. Sources remained unchanged.
- Packaged the template as `.fmp`, imported and ran it through the interface, then disabled it and verified the Run control was disabled.
- Restored two folder tabs after reload and navigated them independently.
- Inspected 1380×880 and 1280×800 layouts, light/dark skins, About, and shortcut help.
- Verified normal/secondary text and accent text contrast across the supplied palettes.

README screenshots are captured from the native WebKit application using generated demonstration files:

- [File browser](screenshots/filemanager-main.png)
- [Plugin center](screenshots/filemanager-plugins.png)
- [Themes](screenshots/filemanager-settings.png)

Earlier local acceptance also covered the three browser views, dual panes, PDF page two, Markdown images/save/leave protection, workspaces, bookmarks, apt inventory, and duplicate recovery. It is not an all-platform certification.

## Packages and upgrade

The final local `FileManager_0.2.0_amd64.deb` and `FileManager_0.2.0_amd64.AppImage` passed isolated native checks for:

1. Startup while the default Shelf shortcut was occupied.
2. Warm instance navigation using a caller-relative directory.
3. Unicode local file URIs.
4. Raw non-UTF-8 paths through single-instance transport.
5. Lazy creation of the native Shelf window.

The installed executable matched the tested deb's executable byte-for-byte; D-Bus reported 0.2.0. Existing catalog/settings values were hash-checked across the update, and legacy state was kept in place. The system's default directory application remained Dolphin.

Local generated package hashes are written to `target/release/bundle/SHA256SUMS`; generated packages and private runtime evidence are not source-control inputs. A source push is separate from a tagged/published GitHub Release.

## Remaining acceptance work

Native Wayland drag/drop, shortcuts, edge detection, portals, scaling/multiple monitors, login/reboot, and broader distribution installation follow the [roadmap](../ROADMAP.md). Signed online plugin distribution and updater mechanisms are not implemented. Short regression tests do not prove days of stable resource use.
