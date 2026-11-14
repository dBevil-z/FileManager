# FileManager 插件开发（API v1）

[English](plugins.md) · [简体中文](plugins.zh-CN.md)

插件是普通 UTF-8 JavaScript 文件，由 Rust 宿主在新的 QuickJS 运行时中执行。无需 Node.js、浏览器、npm 或重新编译 FileManager。内置市场包含四个独立插件；联网市场、发布者签名和自动更新属于后续阶段，见 [路线图](../ROADMAP.zh-CN.md)。

## 从模板开始

```bash
cp -r plugins/template /tmp/my-filemanager-plugin
# 编辑 manifest.json 和 main.js，使用自己的唯一 id
python3 scripts/pack-plugin.py /tmp/my-filemanager-plugin /tmp/hello-1.0.0.fmp
```

在“插件中心 → 导入插件包”中选择 `.fmp`，检查作者、版本、能力与内容摘要，点击“授权并安装”。`.fmp` 本身是 ZIP，只能包含根目录下的 `manifest.json` 和 `main.js`；拒绝额外条目、重复条目、路径穿越、链接及超限文件。整个包不超过 512 KiB，清单 16 KiB，脚本 128 KiB。

`official.` 为自带目录保留的 ID 命名空间；本地插件使用自己的命名空间。

内置官方插件的完整源码位于 [plugins/official](../plugins/official)。它们使用同一套安装和执行接口，没有绕过授权的特权。

## 清单

```json
{
  "schema_version": 1,
  "id": "example.hello",
  "name": "我的插件",
  "version": "1.0.0",
  "description": "清楚说明用途及读取哪些内容。",
  "author": "Your name",
  "category": "效率",
  "permissions": ["context:metadata"],
  "placements": ["tools", "context"],
  "inputs": [
    { "id": "greeting", "label": "问候语", "type": "text", "default": "你好" }
  ]
}
```

- `schema_version` 当前为 1；不兼容的版本拒绝安装。
- `id` 为 3–80 个小写 ASCII 字母、数字、连字符或点，不能包含连续点或以点开头。
- `version` 使用三个非负整数，例如 `1.2.0`。禁止直接降级覆盖，升级重新审核内容和全部权限。
- `placements` 可包含 `tools`、`context`；后者出现在文件右键菜单。
- `inputs` 最多 8 项，类型为 `text` 或 `select`。`select` 需要 `options` 和其中一个 `default`。每项值最多 4096 字节。
- 清单拒绝未知字段。未来扩展通过 API 版本演进，不能依靠未声明特性。

## 执行与返回值

```js
function main(context, api) {
  const entries = context.selection.length
    ? context.selection
    : api.listDirectory(context.directory);
  return {
    title: "文件清单",
    summary: `${entries.length} 个项目`,
    columns: ["名称", "字节"],
    rows: entries.map((file) => [file.name, file.size]),
    reports: [{ name: "清单.json", content: JSON.stringify(entries, null, 2) }],
  };
}
```

`context` 包含 `directory`、`selection`、`inputs`、`api_version`。所选条目包含 `path/name/kind/is_dir/target_is_dir/size/modified` 等元数据。路径可能是不可读的编码令牌，必须原样传回 API，不应拼接或自行解码。

宿主统一渲染表格、摘要和报告，字符串作为文本显示。最多 12 列、5000 行，界面每页渲染 100 行；报告最多 8 个、单个 1 MiB，整体结果 JSON 最多 2 MiB。导出由用户选择目录及名称，拒绝覆盖同名文件，并使用核心日志支持撤销。插件不能自行保存报告或触发剪贴板写入。

API v1 是同步任务模型，不支持 Promise、DOM、自定义 HTML、后台常驻和模块加载。一个入口返回一个结果，关闭后运行时释放。安装但未运行的插件不会创建 JavaScript 引擎或 WebKit 进程。

## 权限与 API

| 权限                       | 能力                                              | 范围与限制                                                                                      |
| -------------------------- | ------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `context:metadata`（必需） | `context.selection`、`api.listDirectory(path)`    | 用户选择的上下文；当前目录或选中目录，一次列出最多 5000 个直属条目。                            |
| `files:read`               | `api.readText(path)`、`api.hash(path, algorithm)` | 选中文件或选中目录树内的普通文件。文本 UTF-8、最多 1 MiB；校验使用 64 KiB 缓冲流式读取。        |
| `directories:scan`         | `api.directoryStats(path)`                        | 当前或选中目录，不跟随符号链接，不跨文件系统；深度 24、最多访问 10 万项、返回最大的 32 个文件。 |

`api.hash(path, 'SHA-256' | 'SHA-512')` 返回 `{hash, bytes}`；默认 SHA-256。校验前后检查文件元数据，变化时拒绝给出“成功”结果。

`api.directoryStats` 返回 `{files, bytes, largest, skipped, truncated}`。统计为文件逻辑大小，不等于实际块占用或独立磁盘空间，硬链接可能分别计数。

所有调用在 Rust 中重新检查权限和路径。脚本不能通过直接调用桥接函数绕过声明。选中目录内指向外部的符号链接不会扩展权限；运行时间和取消状态也在原生读取循环中检查。

## 资源与完整性

- 每次运行使用独立 QuickJS，JS 堆上限 32 MiB，栈上限 512 KiB，任务限时 30 秒。
- CPU 循环和原生 I/O 检查取消；用户可在插件窗口或后台队列取消任务。目录太大时请缩小范围。
- 不提供网络、`process`、`require`、Node.js 标准库、系统命令、DOM 或无范围文件 API。
- 安装确认绑定清单规范化内容和脚本的 SHA-256；每次运行再次验证内容及授权。篡改后停用并要求重新安装。
- 内容摘要表示完整性，不代表发布者身份认证。当前只信任用户明确导入的来源和随应用提供的目录；签名远程市场尚未实现。
- 清单 CSV 插件给电子表格公式前缀加转义，报告导出不覆盖原文件。

当前是进程内受限脚本运行时，属于第一版扩展边界，不应当作对任意恶意原生漏洞的完整操作系统隔离承诺。后续发布远程市场前还需独立安全审查、签名和撤回机制。

## 后续兼容性

先扩展声明式输入和输出，再逐项审查新能力。主题包只开放受限设计令牌；文件写入、解压、重命名等应交给宿主预览确认与核心撤销，不把任意系统访问开放给插件。
