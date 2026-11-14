# Validation record — 0.2.2

[English](verification.md) · [简体中文](verification.zh-CN.md)

Local validation date: **2026-11-14**. The tested environment is Linux x86_64, Ubuntu 24.04, KDE/X11. Ubuntu 26.04 Wayland/KVM and other distributions are planned validation targets, not current certifications. Repository CI status is available in [Actions](https://github.com/dBevil-z/FileManager/actions/workflows/linux.yml).

## Automated checks

| Scope        | Local result | Coverage                                                                                                                                                              |
| ------------ | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust core    | 66 passing   | File conflicts, cross-volume work/undo, legacy databases, cleanup, AI/MCP, preview budgets, plugin boundaries, remote pagination, batch transfers, and sync previews. |
| Tauri/GTK    | 4 passing    | Argument normalization, file URIs, non-UTF-8 paths, deep-link routing, and a separate clipboard test selection.                                                       |
| Frontend     | 11 passing   | Localization, tab restoration/navigation/closing, weighted caches, queued preview cancellation, theme contrast, and GitHub URLs.                                      |
| Static/build | Passing      | TypeScript, Rustfmt, Prettier, whitespace checks, production frontend/native builds, and deb/AppImage packaging.                                                      |

Total: **81 local automated tests** (70 Rust and 11 frontend). The core includes 13 remote-storage tests; the remaining 53 cover jobs, agents, catalogs, files, plugins, previews, processes, and tools. GTK tests use isolated Xvfb, D-Bus, and clipboard selections. Upstream Markdown packages emit harmless empty-directive build warnings.

Plugin tests include traversal, unknown grants, changed installed content, out-of-scope reads, descendant symlink escapes, loop cancellation, JS heap exhaustion, missing host capabilities, and spreadsheet-formula escaping. Atomics/shared buffers/promises are not exposed in the runtime.

## 0.2.2 MinIO accounts and bucket acceptance

Two isolated MinIO services used the default region and custom `lab-region-1`; profiles left region blank. Root and ordinary local IAM usernames/passwords authenticated successfully. An ordinary account paged through 106 buckets; a restricted account listed only its one authorized bucket and was denied access to another. A read-only account lacking `GetBucketLocation` permission also entered its authorized bucket in the custom region.

Scoped recursive upload/download preserved empty folders and verified approximately 9 MB of content without writing into another bucket. Transfer reports and sync previews rejected cross-bucket tokens. Automated fixtures additionally cover ListBuckets continuation tokens, signing-region hints/retries, region XML with declarations, fixed-bucket scope, restricted direct entry, incorrect passwords, and console HTML responses without disclosing credentials or raw server error text.

Browser acceptance used actual sign-in requests for inline errors, empty bucket/region, 106-bucket paging, entry/return/direct navigation, and an English dark layout at 900 pixels.

Both final 0.2.2 deb and AppImage signed in with an ordinary MinIO account in isolated native GTK/WebKit windows, leaving bucket and region blank, then listed buckets and read files from a custom-region bucket. Native D-Bus reported 0.2.2 for each.

## 0.2.1 remote-storage acceptance

Real local MinIO, SFTP, FTP, and explicit FTPS services were tested individually with isolated Home, application state, and credentials:

- A 6,007-entry directory per protocol, read across 44 pages of 137 entries, with no omissions or duplicates; wrong-scope and expired cursors were rejected.
- Batch and recursive directory upload/download, empty directories/files, and a 9 MiB multipart file, with matching round-trip content hashes.
- Existing-file preservation, continued work after individual failures, paginated/filterable reports, and rejection of symlink destination paths.
- Read-only sync previews with missing entries, size/time differences, type/ancestor conflicts, and explicit unverified-content labels for equal sizes.

Rust tests additionally cover cancellation retaining completed files and marking unprocessed entries, temporary-file cleanup, aborting started S3 multipart uploads, OSS V4 signing, profile/credential isolation, replaced upload sources, and preview snapshot boundaries.

Browser checks cover multi-file/folder picking, cross-page selection and navigation, transfer reports, a preview exceeding 6,000 entries, Chinese/English, and a 900-pixel dark layout. The final deb passed native GTK/WebKit file-picker multi-upload and selected download with two byte-identical files. The final AppImage started in extract-and-run mode and successfully browsed SFTP. Actual Alibaba and other cloud buckets still require endpoint-specific acceptance.

## Historical 0.2 memory observations

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

The earlier local 0.2.0 deb/AppImage passed isolated native checks for:

1. Startup while the default Shelf shortcut was occupied.
2. Warm instance navigation using a caller-relative directory.
3. Unicode local file URIs.
4. Raw non-UTF-8 paths through single-instance transport.
5. Lazy creation of the native Shelf window.

The final 0.2.1 deb/AppImage were built and passed the native remote-storage acceptance above. The installed 0.2.0 instance exited through its tray action before a user-level atomic replacement with the tested deb's executable. The new process and D-Bus both report 0.2.1, and the installed executable matches the tested deb byte-for-byte. The previous executable, desktop integration files, and a consistent SQLite snapshot were backed up with a rollback script.

Catalog/settings hashes match across this update. The session, operation history, and legacy data directory remain in place; no fixture connections were added to the user's database. The default directory application remains Dolphin.

The installed 0.2.1 instance subsequently exited through its tray action and was replaced with the tested 0.2.2 deb executable. The new process and D-Bus report 0.2.2 and its executable hash matches the tested package. Catalog, settings, session, and remote-profile hashes and operation-history counts are preserved. A separate backup contains the prior 0.2.1 executable, desktop integration, and consistent SQLite snapshot; Dolphin remains the default directory application.

Local generated package hashes are written to `target/release/bundle/SHA256SUMS`; generated packages and private runtime evidence are not source-control inputs. A source push is separate from a tagged/published GitHub Release.

## Remaining acceptance work

Native Wayland drag/drop, shortcuts, edge detection, portals, scaling/multiple monitors, login/reboot, and broader distribution installation follow the [roadmap](../ROADMAP.md). Signed online plugin distribution and updater mechanisms are not implemented. Short regression tests do not prove days of stable resource use.
