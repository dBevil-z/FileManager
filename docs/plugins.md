# Plugin SDK — API v1

[English](plugins.md) · [简体中文](plugins.zh-CN.md)

Plugins are UTF-8 JavaScript tools executed by the Rust host in a fresh QuickJS runtime. They do not require Node.js, a browser, npm, or an application rebuild. Four included plugins use the same public installation/execution path as locally imported tools. A signed online marketplace is a later [roadmap](../ROADMAP.md) item.

## Start with the template

```bash
cp -r plugins/template /tmp/my-filemanager-plugin
# Edit manifest.json and main.js, using your own unique id.
python3 scripts/pack-plugin.py /tmp/my-filemanager-plugin /tmp/hello-1.0.0.fmp
```

Choose **Plugin Center → Import package**, review the author, version, capabilities, and content digest, then install. A `.fmp` is a ZIP containing only two root entries: `manifest.json` and `main.js`. Extra/duplicate entries, traversal paths, links, and oversized content are rejected. Limits: 512 KiB package, 16 KiB manifest, 128 KiB script.

Included implementations are in [plugins/official](../plugins/official). The `official.` ID namespace is reserved for bundled catalog entries; local plugins must use their own namespace.

## Manifest

```json
{
  "schema_version": 1,
  "id": "example.hello",
  "name": "My plugin",
  "version": "1.0.0",
  "description": "Explain what the tool does and what it reads.",
  "author": "Your name",
  "category": "Productivity",
  "permissions": ["context:metadata"],
  "placements": ["tools", "context"],
  "inputs": [
    {
      "id": "greeting",
      "label": "Greeting",
      "type": "text",
      "default": "Hello"
    }
  ]
}
```

- `schema_version` is currently 1. Incompatible versions are rejected.
- `id` has 3–80 lowercase ASCII letters, digits, hyphens, or dots. It cannot start with a dot or contain consecutive dots.
- `version` uses three non-negative integers, such as `1.2.0`. Direct downgrade replacement is rejected. Updates require a new content/permission review.
- `placements` supports `tools` and `context`; the latter adds a file context-menu entry.
- `inputs` supports at most eight `text`/`select` fields. A select needs `options` and a matching `default`. Input values are limited to 4096 bytes each.
- Unknown manifest fields are rejected. Extend the versioned API rather than relying on undeclared behavior.

## Entry point and output

```js
function main(context, api) {
  const entries = context.selection.length
    ? context.selection
    : api.listDirectory(context.directory);
  return {
    title: "File inventory",
    summary: `${entries.length} entries`,
    columns: ["Name", "Bytes"],
    rows: entries.map((file) => [file.name, file.size]),
    reports: [
      { name: "inventory.json", content: JSON.stringify(entries, null, 2) },
    ],
  };
}
```

`context` contains `directory`, `selection`, `inputs`, and `api_version`. Selected entries expose metadata such as `path`, `name`, `kind`, `is_dir`, `target_is_dir`, `size`, and `modified`. A path can be an opaque token for a non-UTF-8 filename: pass it unchanged to the API rather than joining or decoding it yourself.

The host renders summaries, tables, and reports, treating strings as text. Results allow at most 12 columns and 5000 rows; the UI renders 100 rows per page. Up to eight reports are allowed, each at most 1 MiB, with an overall 2 MiB JSON output limit. Users choose export directories and names; existing files are not overwritten, and exports use the core undo journal. Scripts cannot independently save reports or write the clipboard.

API v1 is synchronous. It has no Promise, DOM, custom HTML, resident background task, or module loader. One invocation returns one result, then its runtime is released. Merely installing a plugin does not create a JavaScript engine or WebKit process.

## Capabilities

| Permission                    | API                                               | Scope and limits                                                                                                                              |
| ----------------------------- | ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `context:metadata` (required) | `context.selection`, `api.listDirectory(path)`    | Current/selected context. A listing reads at most 5000 direct entries of an allowed directory.                                                |
| `files:read`                  | `api.readText(path)`, `api.hash(path, algorithm)` | Regular files explicitly selected or inside a selected directory. UTF-8 text is limited to 1 MiB; hashing uses a 64 KiB streaming buffer.     |
| `directories:scan`            | `api.directoryStats(path)`                        | Current/selected directory; does not follow links or cross filesystems. Depth 24, at most 100,000 visited entries, largest 32 files returned. |

`api.hash(path, 'SHA-256' | 'SHA-512')` returns `{hash, bytes}` and defaults to SHA-256. Metadata is checked before/after hashing; a changed file is not reported as a successful stable checksum.

`api.directoryStats` returns `{files, bytes, largest, skipped, truncated}`. Sizes are logical file sizes, not allocated or uniquely occupied disk blocks; hard links can be counted separately.

The Rust host rechecks permissions and paths on every call. Direct bridge access cannot bypass permission checks. A descendant link to an outside location does not enlarge a selected directory's scope. Native loops also check cancellation and the task deadline between steps.

## Runtime and integrity

- Fresh QuickJS per invocation: 32 MiB JS heap, 512 KiB stack, 30-second task deadline.
- Bytecode loops and native work check cancellation. Tasks can be cancelled in the plugin window or job queue; reduce the scope for large directories.
- No network, `process`, `require`, Node.js standard library, system commands, DOM, or unrestricted filesystem API is exposed.
- Shared buffers, Atomics, and promises are not exposed, avoiding blocking primitives outside bytecode interruption.
- Installation approval binds normalized manifest/script content to a SHA-256 digest. Each run rechecks content and grants; changed files require reinstallation/review.
- A digest proves content integrity, not publisher identity. Only the bundled catalog and explicitly imported local packages are currently supported; remote signing/revocation is not implemented.
- The inventory plugin escapes spreadsheet-formula prefixes. Export refuses name collisions and leaves the source files unchanged.

This first version is an in-process restricted scripting host, not an OS-level isolation guarantee against arbitrary native-engine vulnerabilities. A public remote marketplace requires independent security review and the planned signing/revocation mechanisms.

## Future extensions

Expand declarative inputs/results before adding new capabilities. Theme packs will expose supported visual tokens. File writes, extraction, renaming, and similar actions should use host review, conflict handling, cancellation, and undo rather than unrestricted system access.
