import { useViewState } from "../hooks/useViewState";
import { useViewScroll } from "../hooks/useViewScroll";
import { viewKey } from "../lib/view-memory";
import { getLocale, t } from "../i18n";
import { dragState } from "../lib/drag";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  Grid2X2,
  List,
  Columns3,
  PanelRight,
  Search,
  Plus,
  ChevronRight,
  ChevronDown,
  FolderPlus,
  ArrowDownAZ,
  Eye,
  EyeOff,
  SlidersHorizontal,
  Copy,
  Scissors,
  ClipboardPaste,
  Pencil,
  Trash2,
  Link2,
  Terminal,
  Code2,
  Star,
  RotateCcw,
  Info,
  ExternalLink,
  Layers,
  Palette,
  ArrowUp,
  Check,
  X,
  LayoutGrid,
  FolderOpen,
} from "lucide-react";
import { request, desktop, native } from "../api";
import { useApp } from "../context";
import { useAsync } from "../hooks/useAsync";
import type { Directory, Entry } from "../types";
import {
  basename,
  formatBytes,
  groupLabel,
  isFolder,
  kindLabel,
  shortDate,
} from "../lib/files";
import { FileIcon, SymbolGrid } from "./FileIcon";
import {
  Badge,
  Button,
  Empty,
  ErrorState,
  IconButton,
  Loading,
  Menu,
  Modal,
  Select,
  Field,
  type MenuItem,
} from "./ui";
type View = "icons" | "list" | "columns";
type BrowserProps = {
  path: string;
  memoryKey?: string;
  onNavigate: (path: string) => void;
  active: boolean;
  onActivate: () => void;
  onSelect: (entries: Entry[]) => void;
  reveal?: { path: string; at: number } | null;
  showPreview: boolean;
  togglePreview: () => void;
};
let clipboard: { kind: "copy" | "move"; paths: string[] } | null = null;
export const command = (name: string) =>
  window.dispatchEvent(
    new CustomEvent("filemanager-command", { detail: name }),
  );
export function FileBrowser({
  path,
  memoryKey = "main",
  onNavigate,
  active,
  onActivate,
  onSelect,
  reveal,
  showPreview,
  togglePreview,
}: BrowserProps) {
  const app = useApp();
  const scope = viewKey("folder", memoryKey, path);
  const [view, setView] = useViewState<View>(
    scope + ":view",
    () => (localStorage.getItem("filemanager-view") as View) || "icons",
  );
  const [hidden, setHidden] = useViewState(
    scope + ":hidden",
    () => localStorage.getItem("filemanager-browser-hidden") === "true",
  );
  const [filter, setFilter] = useViewState(scope + ":filter", "");
  const [size, setSize] = useViewState(
    scope + ":size",
    () => localStorage.getItem("filemanager-browser-size") || "medium",
  );
  const [sort, setSort] = useViewState(
    scope + ":sort",
    () => localStorage.getItem("filemanager-browser-sort") || "name",
  );
  const [group, setGroup] = useViewState(
    scope + ":group",
    () => localStorage.getItem("filemanager-browser-group") || "none",
  );
  const locale = getLocale();
  useEffect(() => {
    if (!active) return;
    localStorage.setItem("filemanager-browser-hidden", String(hidden));
    localStorage.setItem("filemanager-browser-size", size);
    localStorage.setItem("filemanager-browser-sort", sort);
    localStorage.setItem("filemanager-browser-group", group);
  }, [active, hidden, size, sort, group]);
  const [selected, setSelected] = useViewState<string[]>(
    scope + ":selected",
    [],
    (v): v is string[] =>
      Array.isArray(v) && v.every((p) => typeof p === "string"),
  );
  const [anchor, setAnchor] = useViewState(scope + ":anchor", "");
  const [columnEntries, setColumnEntries] = useState<Entry[]>([]);
  const [loadingMore, setLoadingMore] = useState(false);
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    items: MenuItem[];
  } | null>(null);
  const [focusSearch, setFocusSearch] = useViewState(
    scope + ":filter-open",
    () => Boolean(filter),
  );
  const [insights, setInsights] = useViewState(scope + ":insights", false);
  const [box, setBox] = useState<{
    x: number;
    y: number;
    endX: number;
    endY: number;
  } | null>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const container = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  const stamp = useRef("");
  const load = useAsync(
    () =>
      request<Directory>("list_directory", {
        path,
        show_hidden: hidden,
        limit: 20000,
      }),
    [path, hidden, app.refresh],
  );
  useViewScroll(
    scroll,
    viewKey(scope, view, size, sort, group, String(hidden)),
    load.data?.path === path,
    undefined,
    Boolean(reveal),
  );
  useEffect(() => {
    if (native) return;
    const timer = window.setInterval(() => {
      if (document.hidden) return;
      void request<{ stamp: string }>("directory_stamp", { path })
        .then((v) => {
          if (stamp.current && stamp.current !== v.stamp) void load.refresh();
          stamp.current = v.stamp;
        })
        .catch(() => {});
    }, 2200);
    return () => window.clearInterval(timer);
  }, [path, load.refresh]);
  useEffect(() => {
    stamp.current = "";
    setColumnEntries([]);
  }, [path]);
  useEffect(() => {
    const node = container.current;
    if (!node) return;
    const observer = new ResizeObserver((v) =>
      setWidth(v[0].contentRect.width),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const entries = useMemo(() => {
    const query = filter.toLowerCase();
    const collator = new Intl.Collator(locale, { numeric: true });
    return (load.data?.entries || [])
      .filter((e) => !query || e.name.toLowerCase().includes(query))
      .sort(
        (a, b) =>
          Number(isFolder(b)) - Number(isFolder(a)) ||
          (sort === "modified"
            ? (b.modified || 0) - (a.modified || 0)
            : sort === "size"
              ? (b.size || 0) - (a.size || 0)
              : sort === "kind"
                ? collator.compare(kindLabel(a), kindLabel(b))
                : 0) ||
          collator.compare(a.name, b.name),
      );
  }, [load.data, filter, sort, locale]);
  const entryIndexes = useMemo(
    () => new Map(entries.map((e, i) => [e.path, i])),
    [entries],
  );
  const selectedPaths = useMemo(() => new Set(selected), [selected]);
  const selection = useMemo(() => {
    const matched = new Map<string, Entry>();
    for (const list of [entries, columnEntries])
      for (const e of list)
        if (selectedPaths.has(e.path)) matched.set(e.path, e);
    return [...matched.values()];
  }, [entries, selectedPaths, columnEntries]);
  useEffect(() => {
    if (active) onSelect(selection);
  }, [selection, active]);
  const select = (
    e: Entry,
    event: Pick<React.MouseEvent, "ctrlKey" | "metaKey" | "shiftKey">,
  ) => {
    onActivate();
    if (event.shiftKey && anchor) {
      const a = entryIndexes.get(anchor);
      const b = entryIndexes.get(e.path);
      if (a !== undefined && b !== undefined)
        setSelected(
          entries.slice(Math.min(a, b), Math.max(a, b) + 1).map((e) => e.path),
        );
    } else if (event.ctrlKey || event.metaKey) {
      setSelected((old) =>
        old.includes(e.path)
          ? old.filter((p) => p !== e.path)
          : [...old, e.path],
      );
      setAnchor(e.path);
    } else {
      setSelected([e.path]);
      setAnchor(e.path);
    }
  };
  const open = (e: Entry) => (isFolder(e) ? onNavigate(e.path) : app.open(e));
  async function newFolder() {
    const name = await app.prompt(t("新建文件夹"), t("未命名文件夹"));
    if (name) {
      try {
        const result = await request<{ entry: Entry }>("create_folder", {
          parent: path,
          name,
        });
        app.refreshAll();
        setSelected([result.entry.path]);
      } catch (e) {
        app.notify(String(e), true);
      }
    }
  }
  async function copy(
    kind: "copy" | "move",
    paths = selection.map((e) => e.path),
  ) {
    if (!paths.length) return;
    const value = { kind, paths };
    if (native) {
      try {
        await desktop("clipboard_write_files", value);
      } catch (e) {
        app.notify(String(e), true);
        return;
      }
    } else clipboard = value;
    app.notify(
      t(
        "{0} {1} 项",
        kind === "copy" ? t("已复制") : t("已剪切"),
        paths.length,
      ),
    );
  }
  async function paste() {
    let value = native ? null : clipboard;
    if (native) {
      try {
        const external = await desktop<{
          paths: string[];
          kind: "copy" | "move";
        }>("clipboard_read_files");
        if (external.paths.length) value = external;
      } catch (e) {
        app.notify(String(e), true);
        return;
      }
    }
    if (!value) return;
    const result = await app.operate(value.kind, value.paths, path);
    if (!native && result && value.kind === "move" && clipboard === value)
      clipboard = null;
  }
  async function rename(items = selection) {
    const selection = items;
    if (!selection.length) return;
    if (selection.length === 1) {
      const name = await app.prompt(t("重新命名"), selection[0].name);
      if (name && name !== selection[0].name) {
        const ext = name.split(".").pop();
        if (
          !isFolder(selection[0]) &&
          ext !== selection[0].extension &&
          !(await app.confirm(
            t("更改文件扩展名？"),
            t("更改扩展名可能影响关联应用。"),
          ))
        )
          return;
        try {
          await request("rename_apply", {
            items: [{ path: selection[0].path, name }],
          });
          app.refreshAll();
        } catch (e) {
          app.notify(String(e), true);
        }
      }
    } else
      app.showModal(
        <BatchRename entries={selection} close={() => app.showModal(null)} />,
      );
  }
  async function trash() {
    if (!selection.length) return;
    await app.operate("trash", selected);
    setSelected([]);
  }
  async function undo() {
    try {
      const result = await request<{
        conflicts: string[];
        count: number;
        message?: string;
      }>("undo");
      app.refreshAll();
      app.notify(
        result.conflicts.length
          ? result.conflicts.join("；")
          : result.message || t("已撤销 {0} 项", result.count),
        result.conflicts.length > 0,
      );
    } catch (e) {
      app.notify(String(e), true);
    }
  }
  const actions = useRef({
    newFolder,
    copy,
    paste,
    rename,
    trash,
    undo,
    peek: () => selection[0] && app.peek(selection[0], entries),
  });
  actions.current = {
    newFolder,
    copy,
    paste,
    rename,
    trash,
    undo,
    peek: () => selection[0] && app.peek(selection[0], entries),
  };
  useEffect(() => {
    if (!active) return;
    const action = (e: Event) => {
      const name = (e as CustomEvent<string>).detail;
      if (name === "new") void actions.current.newFolder();
      if (name === "copy") void actions.current.copy("copy");
      if (name === "cut") void actions.current.copy("move");
      if (name === "paste") void actions.current.paste();
      if (name === "rename") void actions.current.rename();
      if (name === "trash") void actions.current.trash();
      if (name === "undo") void actions.current.undo();
      if (name === "peek") actions.current.peek();
      if (name === "path")
        window.dispatchEvent(new CustomEvent("filemanager-edit-path"));
    };
    window.addEventListener("filemanager-command", action);
    return () => window.removeEventListener("filemanager-command", action);
  }, [active]);
  useEffect(() => {
    if (!active) return;
    const key = (event: KeyboardEvent) => {
      if (
        (event.target as HTMLElement).closest(
          'input,textarea,[contenteditable="true"],.modal,.peek-backdrop',
        )
      )
        return;
      const ctrl = event.ctrlKey || event.metaKey;
      if (ctrl && ["a", "f", "h"].includes(event.key.toLowerCase())) {
        event.preventDefault();
        switch (event.key.toLowerCase()) {
          case "a":
            setSelected(entries.map((e) => e.path));
            break;
          case "c":
            void copy("copy");
            break;
          case "x":
            void copy("move");
            break;
          case "v":
            void paste();
            break;
          case "z":
            void undo();
            break;
          case "f":
            setFocusSearch(true);
            break;
          case "h":
            setHidden((v) => !v);
            break;
          case "n":
            void newFolder();
            break;
        }
        return;
      }
      if (event.key === "Enter" && selection[0]) {
        event.preventDefault();
        open(selection[0]);
      } else if (event.key === "Backspace" && load.data?.parent) {
        event.preventDefault();
        onNavigate(load.data.parent);
      } else if (event.altKey && event.key === "ArrowUp" && load.data?.parent) {
        event.preventDefault();
        onNavigate(load.data.parent);
      } else if (
        ["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight"].includes(event.key)
      ) {
        event.preventDefault();
        const current = entries.findIndex((e) => e.path === selected.at(-1));
        const cols =
          view === "icons"
            ? Math.max(
                1,
                Math.floor(
                  (width - 40) /
                    (size === "small" ? 135 : size === "large" ? 240 : 190),
                ),
              )
            : 1;
        const delta =
          event.key === "ArrowDown"
            ? cols
            : event.key === "ArrowUp"
              ? -cols
              : event.key === "ArrowLeft"
                ? -1
                : 1;
        const index = Math.max(
          0,
          Math.min(entries.length - 1, current < 0 ? 0 : current + delta),
        );
        if (entries[index]) {
          select(entries[index], {
            ctrlKey: false,
            metaKey: false,
            shiftKey: event.shiftKey,
          });
          container.current
            ?.querySelector(`[data-index="${index}"]`)
            ?.scrollIntoView({ block: "nearest" });
        }
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [active, entries, selection, path, selected, width, size, view]);
  function context(event: React.MouseEvent, entry?: Entry) {
    event.preventDefault();
    event.stopPropagation();
    onActivate();
    const items: Entry[] =
      entry && !selected.includes(entry.path) ? [entry] : selection;
    if (entry && !selected.includes(entry.path)) setSelected([entry.path]);
    const paths = items.map((e) => e.path);
    const target = entry || items[0];
    const menuItems: MenuItem[] = target
      ? [
          {
            label: t("打开"),
            icon: <ExternalLink size={15} />,
            action: () => open(target),
          },
          {
            label: t("在新标签页打开"),
            disabled: !isFolder(target),
            action: () => app.openTab(target.path),
          },
          {
            label: t("快速预览"),
            icon: <Eye size={15} />,
            key: "Space",
            action: () => app.peek(target, entries),
          },
          {
            label: t("选择应用打开…"),
            action: () =>
              void desktop("open_with", { path: target.path }).catch((e) =>
                app.notify(String(e), true),
              ),
          },
          {
            label: t("在 Dolphin 中显示"),
            action: () =>
              void desktop("reveal", { path: target.path }).catch((e) =>
                app.notify(String(e), true),
              ),
          },
          { separator: true, label: "" },
          {
            label: t("复制"),
            icon: <Copy size={15} />,
            key: "Ctrl C",
            action: () => void copy("copy", paths),
          },
          {
            label: t("剪切"),
            icon: <Scissors size={15} />,
            key: "Ctrl X",
            action: () => void copy("move", paths),
          },
          {
            label: t("粘贴"),
            icon: <ClipboardPaste size={15} />,
            key: "Ctrl V",
            action: () => void paste(),
          },
          {
            label: t("创建符号链接…"),
            action: async () => {
              const directories = await app.pickFiles({ directories: true });
              if (directories[0])
                await app.operate("symlink", paths, directories[0]);
            },
          },
          {
            label: t("分享…"),
            action: () =>
              app.showModal(
                <Share entry={target} close={() => app.showModal(null)} />,
              ),
          },
          {
            label: t("制作副本"),
            action: () => void app.operate("duplicate", paths),
          },
          {
            label: t("复制路径"),
            icon: <Link2 size={15} />,
            action: () => void app.saveText(paths.join("\n")),
          },
          {
            label: t("重新命名…"),
            icon: <Pencil size={15} />,
            key: "F2",
            action: () => void rename(items),
          },
          ...(isFolder(target)
            ? [
                {
                  label: t("加入收藏"),
                  icon: <Star size={15} />,
                  action: () =>
                    void app.updateCatalog((v) => ({
                      ...v,
                      favourites: [
                        ...v.favourites.filter((f) => f.path !== target.path),
                        {
                          id: crypto.randomUUID(),
                          name: target.name,
                          path: target.path,
                        },
                      ],
                    })),
                },
                {
                  label: t("颜色与符号…"),
                  icon: <Palette size={15} />,
                  action: () =>
                    app.showModal(
                      <FolderStyle
                        entry={target}
                        close={() => app.showModal(null)}
                      />,
                    ),
                },
              ]
            : []),
          {
            label: t("添加到 Holding"),
            icon: <Layers size={15} />,
            action: () =>
              void app.updateCatalog((v) => ({
                ...v,
                holding: [...new Set([...v.holding, ...paths])],
              })),
          },
          {
            label: t("在终端打开"),
            icon: <Terminal size={15} />,
            action: () =>
              void desktop("terminal", {
                path: target.path,
                app: app.settings.terminal,
              }).catch((e) => app.notify(String(e), true)),
          },
          {
            label: t("在编辑器打开"),
            icon: <Code2 size={15} />,
            action: () =>
              void desktop("editor", {
                path: target.path,
                app: app.settings.editor,
              }).catch((e) => app.notify(String(e), true)),
          },
          { separator: true, label: "" },
          {
            label: t("移到回收站"),
            icon: <Trash2 size={15} />,
            key: "Delete",
            danger: true,
            action: () => void app.operate("trash", paths),
          },
        ]
      : [
          {
            label: t("新建文件夹…"),
            icon: <FolderPlus size={15} />,
            key: "Ctrl N",
            action: () => void newFolder(),
          },
          {
            label: t("粘贴"),
            icon: <ClipboardPaste size={15} />,
            action: () => void paste(),
          },
          {
            label: t("撤销上次操作"),
            icon: <RotateCcw size={15} />,
            action: () => void undo(),
          },
          {
            label: hidden ? t("隐藏点文件") : t("显示隐藏文件"),
            icon: hidden ? <EyeOff size={15} /> : <Eye size={15} />,
            action: () => setHidden((v) => !v),
          },
          {
            label: t("在终端打开"),
            icon: <Terminal size={15} />,
            action: () =>
              void desktop("terminal", { path, app: app.settings.terminal }),
          },
        ];
    const extensions = app.plugins.filter(
      (p) => p.enabled && p.manifest.placements.includes("context"),
    );
    if (extensions.length)
      menuItems.push(
        { label: "", separator: true },
        ...extensions.map((p) => ({
          label: `插件 · ${p.manifest.name}`,
          action: () => app.runPlugin(p.manifest.id, paths, path),
        })),
      );
    menuItems.push({
      label: t("管理插件…"),
      action: () => app.navigate({ type: "plugins" }),
    });
    setMenu({ x: event.clientX, y: event.clientY, items: menuItems });
  }
  const columns =
    view === "icons"
      ? Math.max(
          1,
          Math.floor(
            (width - 44) /
              (size === "small" ? 130 : size === "large" ? 248 : 160),
          ),
        )
      : 1;
  const rows = useMemo(() => {
    const groups = new Map<string, Entry[]>();
    for (const entry of entries) {
      const label = isFolder(entry)
        ? t("文件夹")
        : group === "type"
          ? groupLabel(entry)
          : group === "date"
            ? shortDate(entry.modified).replace(/ \d+:\d+$/, "")
            : t("文件");
      const bucket = groups.get(label);
      if (bucket) bucket.push(entry);
      else groups.set(label, [entry]);
    }
    const result: { heading?: string; count?: number; entries?: Entry[] }[] =
      [];
    for (const [heading, values] of groups) {
      if (view !== "list" || group !== "none")
        result.push({ heading, count: values.length });
      for (let i = 0; i < values.length; i += columns)
        result.push({ entries: values.slice(i, i + columns) });
    }
    return result;
  }, [entries, group, view, columns, locale]);
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroll.current,
    estimateSize: (i) =>
      rows[i]?.heading
        ? 40
        : view === "list"
          ? 38
          : size === "large"
            ? 240
            : size === "small"
              ? 150
              : 180,
    overscan: 4,
  });
  const virtualized = entries.length > 300;
  const revealed = useRef(0);
  useEffect(() => {
    if (
      !reveal ||
      !active ||
      revealed.current === reveal.at ||
      !entryIndexes.has(reveal.path)
    )
      return;
    revealed.current = reveal.at;
    setSelected([reveal.path]);
    setAnchor(reveal.path);
    if (virtualized && view !== "columns") {
      const index = rows.findIndex((row) =>
        row.entries?.some((item) => item.path === reveal.path),
      );
      if (index >= 0) virtual.scrollToIndex(index, { align: "center" });
    } else {
      requestAnimationFrame(() => {
        const root = container.current;
        const node =
          root &&
          Array.from(root.querySelectorAll<HTMLElement>("[data-path]")).find(
            (item) => item.dataset.path === reveal.path,
          );
        node?.scrollIntoView({ block: "nearest" });
      });
    }
  }, [active, reveal, entryIndexes, rows, virtual, virtualized, view]);
  function drag(event: React.DragEvent, entry: Entry) {
    const paths = selected.includes(entry.path) ? selected : [entry.path];
    event.dataTransfer.setData(
      "application/x-filemanager-paths",
      JSON.stringify(paths),
    );
    event.dataTransfer.setData(
      "text/uri-list",
      paths
        .filter((p) => p.startsWith("/"))
        .map((p) => `file://${encodeURI(p)}`)
        .join("\r\n"),
    );
    event.dataTransfer.effectAllowed = "copyMove";
    if (native) {
      event.preventDefault();
      dragState.paths = paths;
      dragState.kind = event.ctrlKey ? "copy" : "move";
      void desktop("drag_files", { paths }).catch((e) =>
        app.notify(String(e), true),
      );
    }
  }
  async function drop(event: React.DragEvent, destination = path) {
    event.preventDefault();
    event.stopPropagation();
    const raw = event.dataTransfer.getData("application/x-filemanager-paths");
    if (raw) {
      try {
        await app.operate(
          event.ctrlKey ? "copy" : "move",
          JSON.parse(raw),
          destination,
        );
      } catch (e) {
        app.notify(String(e), true);
      }
    }
  }
  const rowElement = (
    row: (typeof rows)[number],
    index: number,
    style?: React.CSSProperties,
  ) =>
    row.heading ? (
      <div key={`h-${index}`} className="file-group-title" style={style}>
        <ChevronDown size={12} />
        <span>{row.heading}</span>
        <span>{row.count}</span>
      </div>
    ) : (
      <div
        key={index}
        className={`file-row ${view}`}
        style={{
          ...style,
          gridTemplateColumns:
            view === "icons" ? `repeat(${columns}, minmax(0,1fr))` : undefined,
        }}
      >
        {row.entries?.map((entry) => (
          <div
            key={entry.path}
            role="option"
            aria-selected={selectedPaths.has(entry.path)}
            tabIndex={-1}
            data-path={entry.path}
            data-drop-path={isFolder(entry) ? entry.path : undefined}
            data-index={entryIndexes.get(entry.path)}
            className={`file-item ${view} ${selectedPaths.has(entry.path) ? "selected" : ""} size-${size}`}
            onClick={(e) => select(entry, e)}
            onDoubleClick={() => open(entry)}
            onContextMenu={(e) => context(e, entry)}
            draggable
            onDragStart={(e) => drag(e, entry)}
            onDragOver={(e) => {
              if (isFolder(entry)) {
                e.preventDefault();
                e.currentTarget.classList.add("drop-target");
              }
            }}
            onDragLeave={(e) => e.currentTarget.classList.remove("drop-target")}
            onDrop={(e) => {
              e.currentTarget.classList.remove("drop-target");
              if (isFolder(entry)) void drop(e, entry.path);
            }}
          >
            <FileIcon entry={entry} thumbnail={view === "icons"} />
            <div className="file-name" title={entry.name}>
              {entry.name}
            </div>
            {view === "icons" ? (
              <div className="file-subtitle">
                {isFolder(entry) ? kindLabel(entry) : formatBytes(entry.size)}
              </div>
            ) : (
              <>
                <span className="file-date">{shortDate(entry.modified)}</span>
                <span className="file-kind">{kindLabel(entry)}</span>
                <span className="file-size">{formatBytes(entry.size)}</span>
              </>
            )}
          </div>
        ))}
      </div>
    );
  return (
    <section
      className={`browser-pane ${active ? "active-pane" : ""}`}
      ref={container}
      onPointerDown={onActivate}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => void drop(e)}
    >
      <div className="browser-tools">
        <div className="segmented">
          {(
            [
              ["icons", Grid2X2, t("图标视图")],
              ["list", List, t("列表视图")],
              ["columns", Columns3, t("分栏视图")],
            ] as const
          ).map(([value, Icon, label]) => (
            <IconButton
              key={value}
              label={label}
              active={view === value}
              onClick={() => {
                setView(value);
                localStorage.setItem("filemanager-view", value);
              }}
            >
              <Icon size={15} />
            </IconButton>
          ))}
        </div>
        <span className="toolbar-separator" />
        <IconButton
          label={t("新建文件夹 (Ctrl N)")}
          onClick={() => void newFolder()}
        >
          <FolderPlus size={16} />
        </IconButton>
        <div className="flex-1" />
        <IconButton
          label={t("目录容量与整理")}
          active={insights}
          onClick={() => setInsights((v) => !v)}
        >
          <SlidersHorizontal size={16} />
        </IconButton>
        <Select
          value={group}
          onChange={(e) => setGroup(e.target.value)}
          aria-label={t("分组")}
        >
          <option value="none">{t("不分组")}</option>
          <option value="type">{t("按种类分组")}</option>
          <option value="date">{t("按日期分组")}</option>
        </Select>
        <IconButton
          label={t("排序与显示")}
          onClick={(e) =>
            setMenu({
              x: e.clientX - 170,
              y: e.clientY + 20,
              items: [
                { label: t("按名称排序"), action: () => setSort("name") },
                {
                  label: t("按修改日期排序"),
                  action: () => setSort("modified"),
                },
                { label: t("按大小排序"), action: () => setSort("size") },
                { separator: true, label: "" },
                ...["small", "medium", "large"].map((v, i) => ({
                  label: t(
                    "{0}{1}",
                    size === v ? "✓ " : "",
                    [t("小图标"), t("中图标"), t("大图标")][i],
                  ),
                  action: () => setSize(v),
                })),
                {
                  label: t("{0}显示隐藏文件", hidden ? "✓ " : ""),
                  action: () => setHidden((v) => !v),
                },
              ],
            })
          }
        >
          <ArrowDownAZ size={16} />
        </IconButton>
        <IconButton
          label={t("筛选文件 (Ctrl F)")}
          active={focusSearch}
          onClick={() => setFocusSearch((v) => !v)}
        >
          <Search size={16} />
        </IconButton>
        <IconButton
          label={t("侧边预览")}
          active={showPreview}
          onClick={togglePreview}
        >
          <PanelRight size={16} />
        </IconButton>
      </div>
      <PathBar
        path={path}
        displayPath={load.data?.display_path}
        onNavigate={onNavigate}
        active={active}
      />
      {focusSearch && (
        <div className="browser-filter">
          <Search size={15} />
          <input
            autoFocus
            placeholder={t("在此文件夹中筛选…")}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <IconButton
            label={t("结束筛选")}
            onClick={() => {
              setFilter("");
              setFocusSearch(false);
            }}
          >
            <X size={14} />
          </IconButton>
        </div>
      )}
      {insights && <Insights path={path} />}
      {view === "list" && (
        <div className="list-heading">
          <span>{t("名称")}</span>
          <span>{t("修改日期")}</span>
          <span>{t("种类")}</span>
          <span>{t("大小")}</span>
        </div>
      )}
      {load.loading && !load.data ? (
        <Loading />
      ) : load.error ? (
        <div className="path-error">
          <ErrorState error={load.error} retry={() => void load.refresh()} />
          <Button
            onClick={async () => {
              const pieces = path.split("/");
              const name = pieces.pop();
              if (name && (await app.confirm(t("创建缺失的目录？"), path))) {
                try {
                  await request("create_folder", {
                    parent: pieces.join("/") || "/",
                    name,
                  });
                  void load.refresh();
                } catch (e) {
                  app.notify(String(e), true);
                }
              }
            }}
          >
            {t("创建此目录")}
          </Button>
        </div>
      ) : view === "columns" ? (
        <Columns
          key={path}
          memoryKey={scope}
          onEntries={setColumnEntries}
          hidden={hidden}
          entries={entries}
          selected={selected}
          select={select}
          open={open}
          context={context}
        />
      ) : (
        <div
          className="file-scroll"
          ref={scroll}
          role="listbox"
          aria-label={basename(path)}
          aria-multiselectable="true"
          onContextMenu={(e) => {
            if (!(e.target as HTMLElement).closest(".file-item")) context(e);
          }}
          onMouseDown={(e) => {
            if (e.button || (e.target as HTMLElement).closest(".file-item"))
              return;
            if (!e.ctrlKey && !e.metaKey) setSelected([]);
            const x = e.clientX,
              y = e.clientY;
            setBox({ x, y, endX: x, endY: y });
            const move = (v: MouseEvent) => {
              setBox({ x, y, endX: v.clientX, endY: v.clientY });
              if (Math.abs(v.clientX - x) + Math.abs(v.clientY - y) < 5) return;
              const paths: string[] = [];
              scroll.current
                ?.querySelectorAll<HTMLElement>(".file-item")
                .forEach((node) => {
                  const r = node.getBoundingClientRect();
                  if (
                    r.right >= Math.min(x, v.clientX) &&
                    r.left <= Math.max(x, v.clientX) &&
                    r.bottom >= Math.min(y, v.clientY) &&
                    r.top <= Math.max(y, v.clientY)
                  )
                    paths.push(node.dataset.path!);
                });
              setSelected(paths);
            };
            const up = () => {
              setBox(null);
              window.removeEventListener("mousemove", move);
              window.removeEventListener("mouseup", up);
            };
            window.addEventListener("mousemove", move);
            window.addEventListener("mouseup", up);
          }}
        >
          {!entries.length ? (
            <Empty
              icon={<FolderOpen size={34} />}
              title={filter ? t("没有匹配的文件") : t("这个文件夹是空的")}
            >
              {filter ? (
                t("试试其他名称。")
              ) : (
                <>
                  <p>{t("将文件拖到这里，或创建一个文件夹。")}</p>
                  <Button onClick={() => void newFolder()}>
                    <Plus size={14} />
                    {t("新建文件夹")}
                  </Button>
                </>
              )}
            </Empty>
          ) : virtualized ? (
            <div
              style={{ height: virtual.getTotalSize(), position: "relative" }}
            >
              {virtual.getVirtualItems().map((row) =>
                rowElement(rows[row.index], row.index, {
                  position: "absolute",
                  width: "100%",
                  top: 0,
                  transform: `translateY(${row.start}px)`,
                  height: row.size,
                }),
              )}
            </div>
          ) : (
            rows.map((row, i) => rowElement(row, i))
          )}
        </div>
      )}
      <footer className="pane-status">
        <span>
          {selection.length
            ? t(
                "已选 {0} 项 · {1}",
                selection.length,
                formatBytes(selection.reduce((n, e) => n + (e.size || 0), 0)),
              )
            : t("{0} 个项目", load.data?.total || 0)}
        </span>
        <span className="flex-1" />
        {load.data && load.data.entries.length < load.data.total && (
          <Button
            disabled={loadingMore}
            onClick={async () => {
              setLoadingMore(true);
              const current = load.data;
              if (!current) {
                setLoadingMore(false);
                return;
              }
              try {
                const next = await request<Directory>("list_directory", {
                  path,
                  show_hidden: hidden,
                  offset: current.entries.length,
                  limit: 20000,
                });
                load.setData((value) =>
                  value?.path === next.path
                    ? { ...next, entries: [...value.entries, ...next.entries] }
                    : value,
                );
              } catch (e) {
                app.notify(String(e), true);
              } finally {
                setLoadingMore(false);
              }
            }}
          >
            {t("载入更多 (")}
            {load.data.entries.length}/{load.data.total})
          </Button>
        )}
        {hidden && <Eye size={12} />}
        <span>
          {view === "icons"
            ? t("图标")
            : view === "list"
              ? t("列表")
              : t("分栏")}
        </span>
        {load.data && !load.data.writable && <Badge>{t("只读")}</Badge>}
      </footer>
      {menu && <Menu {...menu} close={() => setMenu(null)} />}
      {box && (
        <div
          className="selection-box"
          style={{
            left: Math.min(box.x, box.endX),
            top: Math.min(box.y, box.endY),
            width: Math.abs(box.endX - box.x),
            height: Math.abs(box.endY - box.y),
          }}
        />
      )}
    </section>
  );
}
function PathBar({
  path,
  displayPath,
  onNavigate,
  active,
}: {
  path: string;
  displayPath?: string;
  onNavigate: (path: string) => void;
  active: boolean;
}) {
  const app = useApp();
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(path);
  const [options, setOptions] = useState<{ path: string; name: string }[]>([]);
  useEffect(() => setValue(path), [path]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (active && (e.ctrlKey || e.metaKey) && e.key === "l") {
        e.preventDefault();
        setEditing(true);
      }
    };
    const edit = () => active && setEditing(true);
    window.addEventListener("keydown", key);
    window.addEventListener("filemanager-edit-path", edit);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("filemanager-edit-path", edit);
    };
  }, [active]);
  useEffect(() => {
    if (!editing) return;
    const localValue = setTimeout(
      () =>
        void request<{ items: { path: string; name: string }[] }>(
          "path_complete",
          { text: value },
        )
          .then((v) => setOptions(v.items || []))
          .catch(() => setOptions([])),
      120,
    );
    return () => clearTimeout(localValue);
  }, [editing, value]);
  const base = [...app.boot.places]
    .filter(
      (place) =>
        place.path !== "/" &&
        (path === place.path || path.startsWith(place.path + "/")),
    )
    .sort((a, b) => b.path.length - a.path.length)[0];
  const opaque = path.startsWith("fileos:bytes:");
  const tail = (base ? path.slice(base.path.length) : path)
    .split("/")
    .filter(Boolean);
  const crumbs = opaque
    ? [{ name: displayPath ? basename(displayPath) : t("目录"), path }]
    : [
        ...(base ? [{ name: t(base.name), path: base.path }] : []),
        ...tail.map((name, i) => ({
          name,
          path: `${base?.path || ""}/${tail.slice(0, i + 1).join("/")}`,
        })),
      ];
  return (
    <div className="path-bar" onDoubleClick={() => setEditing(true)}>
      {editing ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            const aliases: Record<string, string> = {
              term: "terminal",
              terminal: "terminal",
              code: "editor",
              editor: "editor",
              cursor: "editor",
              zed: "editor",
              idea: "editor",
            };
            const match = value
              .trim()
              .match(
                /^(term|terminal|code|editor|cursor|zed|idea)(?::\s*(.*))?$/i,
              );
            const alias =
              match?.[1].toLowerCase() || value.trim().toLowerCase();
            const aliasPath = match?.[2] || path;
            if (aliases[alias])
              void desktop(aliases[alias], {
                path: aliasPath,
                app:
                  aliases[alias] === "terminal"
                    ? app.settings.terminal
                    : ["code", "cursor", "zed", "idea"].includes(alias)
                      ? alias
                      : app.settings.editor,
              }).catch((e) => app.notify(String(e), true));
            else onNavigate(value);
            setEditing(false);
          }}
        >
          <FolderOpen size={14} />
          <input
            autoFocus
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setEditing(false);
                setValue(path);
              }
              if (e.key === "Tab" && options.length) {
                e.preventDefault();
                setValue(options[0].path + "/");
              }
            }}
            onBlur={() => setTimeout(() => setEditing(false), 150)}
          />
          {options.length > 0 && (
            <div className="path-options">
              {options.slice(0, 8).map((v) => (
                <button
                  type="button"
                  key={v.path}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => {
                    onNavigate(v.path);
                    setEditing(false);
                  }}
                >
                  <FolderOpen size={14} />
                  {v.path}
                </button>
              ))}
            </div>
          )}
        </form>
      ) : (
        <>
          <button title={t("文件系统")} onClick={() => onNavigate("/")}>
            <LayoutGrid size={13} />
          </button>
          {crumbs.map((part) => (
            <span key={part.path}>
              <ChevronRight size={11} />
              <button title={part.name} onClick={() => onNavigate(part.path)}>
                {part.name}
              </button>
            </span>
          ))}
          <button
            className="path-edit"
            title={t("输入路径 (Ctrl L)")}
            onClick={() => setEditing(true)}
          >
            ⌘
          </button>
        </>
      )}
    </div>
  );
}
function Columns({
  memoryKey,
  entries,
  selected,
  select,
  open,
  context,
  onEntries,
  hidden,
}: {
  memoryKey: string;
  entries: Entry[];
  selected: string[];
  select: (entry: Entry, event: React.MouseEvent) => void;
  open: (entry: Entry) => void;
  context: (event: React.MouseEvent, entry: Entry) => void;
  onEntries: (entries: Entry[]) => void;
  hidden: boolean;
}) {
  const [trail, setTrail] = useViewState<Entry[]>(
    memoryKey + ":columns",
    [],
    (v): v is Entry[] =>
      Array.isArray(v) &&
      v.length <= 64 &&
      v.every(
        (e) =>
          e &&
          typeof e.path === "string" &&
          typeof e.name === "string" &&
          typeof e.is_dir === "boolean",
      ),
  );
  const loaded = useRef(new Map<string, Entry[]>());
  const root = useRef<HTMLDivElement>(null);
  useViewScroll(root, memoryKey + ":column-scroll");
  const previousDepth = useRef(trail.length);
  const choose = (entry: Entry, event: React.MouseEvent, level: number) => {
    select(entry, event);
    setTrail((old) => [
      ...old.slice(0, level),
      ...(isFolder(entry) ? [entry] : []),
    ]);
  };
  useEffect(() => {
    if (previousDepth.current === trail.length) return;
    previousDepth.current = trail.length;
    root.current?.scrollTo({
      left: root.current.scrollWidth,
      behavior: "smooth",
    });
  }, [trail.length]);
  const report = useCallback(
    (path: string, rows: Entry[]) => {
      loaded.current.set(path, rows);
      onEntries([...loaded.current.values()].flat());
    },
    [onEntries],
  );
  const selectedPaths = useMemo(() => new Set(selected), [selected]);
  const render = (entry: Entry, level: number) => (
    <button
      key={entry.path}
      data-path={entry.path}
      data-drop-path={isFolder(entry) ? entry.path : undefined}
      className={
        selectedPaths.has(entry.path) || trail[level]?.path === entry.path
          ? "selected"
          : ""
      }
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData(
          "text/uri-list",
          `file://${encodeURI(entry.path)}`,
        );
        event.dataTransfer.setData(
          "application/x-filemanager-paths",
          JSON.stringify([entry.path]),
        );
        dragState.paths = [entry.path];
        dragState.kind = event.altKey ? "copy" : "move";
        if (native)
          void desktop("drag_files", { paths: [entry.path] }).catch(() => {});
      }}
      onClick={(event) => choose(entry, event, level)}
      onDoubleClick={() => open(entry)}
      onContextMenu={(event) => context(event, entry)}
    >
      <FileIcon entry={entry} />
      <span>{entry.name}</span>
      {isFolder(entry) && <ChevronRight size={13} />}
    </button>
  );
  return (
    <div className="columns-view" ref={root}>
      <ColumnRows entries={entries}>{(entry) => render(entry, 0)}</ColumnRows>
      {trail.map((folder, index) => (
        <DirectoryColumn
          key={folder.path}
          folder={folder}
          hidden={hidden}
          report={report}
        >
          {(entry) => render(entry, index + 1)}
        </DirectoryColumn>
      ))}
    </div>
  );
}
function ColumnRows({
  entries,
  children,
  footer,
}: {
  entries: Entry[];
  children: (entry: Entry) => React.ReactNode;
  footer?: React.ReactNode;
}) {
  const scroll = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    count: entries.length,
    getScrollElement: () => scroll.current,
    estimateSize: () => 31,
    overscan: 8,
    enabled: entries.length > 150,
  });
  return (
    <div className="column-list" ref={scroll}>
      {entries.length > 150 ? (
        <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
          {virtual.getVirtualItems().map((row) => (
            <div
              key={entries[row.index].path}
              style={{
                position: "absolute",
                width: "100%",
                transform: `translateY(${row.start}px)`,
              }}
            >
              {children(entries[row.index])}
            </div>
          ))}
        </div>
      ) : (
        entries.map(children)
      )}
      {footer}
    </div>
  );
}
function DirectoryColumn({
  folder,
  hidden,
  report,
  children,
}: {
  folder: Entry;
  hidden: boolean;
  report: (path: string, rows: Entry[]) => void;
  children: (entry: Entry) => React.ReactNode;
}) {
  const app = useApp();
  const data = useAsync(
    () =>
      request<Directory>("list_directory", {
        path: folder.path,
        show_hidden: hidden,
        limit: 20000,
      }),
    [folder.path, hidden, app.refresh],
  );
  useEffect(() => {
    report(folder.path, data.data?.entries || []);
    return () => report(folder.path, []);
  }, [folder.path, data.data, report]);
  if (data.loading || data.error) {
    return (
      <div className="column-list">
        {data.loading ? <Loading /> : <ErrorState error={data.error} />}
      </div>
    );
  }
  const footer = data.data && data.data.entries.length < data.data.total && (
    <Button
      onClick={async () => {
        try {
          const current = data.data;
          if (!current) return;
          const more = await request<Directory>("list_directory", {
            path: folder.path,
            show_hidden: hidden,
            limit: 20000,
            offset: current.entries.length,
          });
          data.setData((old) =>
            old?.path === more.path
              ? { ...more, entries: [...old.entries, ...more.entries] }
              : old,
          );
        } catch (e) {
          app.notify(String(e), true);
        }
      }}
    >
      {t("载入更多")}
    </Button>
  );
  return (
    <ColumnRows entries={data.data?.entries || []} footer={footer}>
      {children}
    </ColumnRows>
  );
}
function BatchRename({
  entries,
  close,
}: {
  entries: Entry[];
  close: () => void;
}) {
  const app = useApp();
  const [mode, setMode] = useState("replace");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [text, setText] = useState(t("文件"));
  const [start, setStart] = useState(1);
  const [keep, setKeep] = useState(true);
  const args = {
    paths: entries.map((e) => e.path),
    mode,
    find: from,
    text: mode === "replace" ? to : text,
    start,
    keep_extension: keep,
  };
  const plan = useAsync(
    () =>
      request<{
        items: {
          path: string;
          name: string;
          old_name: string;
          error?: string;
        }[];
        valid: boolean;
      }>("rename_plan", args),
    [mode, from, to, text, start, keep],
  );
  return (
    <Modal
      title={t("重新命名 {0} 个项目", entries.length)}
      wide
      onClose={close}
      footer={
        <>
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={!plan.data?.valid}
            onClick={async () => {
              try {
                await request("rename_apply", { items: plan.data?.items });
                app.refreshAll();
                close();
              } catch (e) {
                app.notify(String(e), true);
              }
            }}
          >
            {t("重新命名")}
          </Button>
        </>
      }
    >
      <div className="form-row">
        <Select value={mode} onChange={(e) => setMode(e.target.value)}>
          <option value="replace">{t("替换文字")}</option>
          <option value="prefix">{t("添加前缀")}</option>
          <option value="suffix">{t("添加后缀")}</option>
          <option value="number">{t("编号")}</option>
        </Select>
        {mode === "replace" ? (
          <>
            <input
              placeholder={t("查找")}
              value={from}
              onChange={(e) => setFrom(e.target.value)}
            />
            <input
              placeholder={t("替换为")}
              value={to}
              onChange={(e) => setTo(e.target.value)}
            />
          </>
        ) : (
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("文字")}
          />
        )}
        {mode === "number" && (
          <input
            type="number"
            value={start}
            onChange={(e) => setStart(Number(e.target.value))}
          />
        )}
      </div>
      <label className="check-label">
        <input
          type="checkbox"
          checked={keep}
          onChange={(e) => setKeep(e.target.checked)}
        />
        {t("保留扩展名")}
      </label>
      <div className="rename-preview">
        {plan.data?.items?.map((e) => (
          <div key={e.path}>
            <span>{e.old_name}</span>
            <ChevronRight size={14} />
            <strong className={e.error ? "danger-text" : ""}>{e.name}</strong>
            {e.error && <Badge>{e.error}</Badge>}
          </div>
        ))}
      </div>
      {plan.error && <p className="danger-text">{plan.error}</p>}
    </Modal>
  );
}
function FolderStyle({ entry, close }: { entry: Entry; close: () => void }) {
  const app = useApp();
  const current = app.catalog.folderStyles[entry.path] || {};
  const [color, setColor] = useState(current.color || app.settings.folderColor);
  const [symbol, setSymbol] = useState(current.symbol || "");
  return (
    <Modal
      title={t("自定义「{0}」", entry.name)}
      onClose={close}
      footer={
        <>
          <Button
            onClick={async () => {
              await app.updateCatalog((v) => {
                const styles = { ...v.folderStyles };
                delete styles[entry.path];
                return { ...v, folderStyles: styles };
              });
              close();
            }}
          >
            {t("恢复自动")}
          </Button>
          <Button
            tone="primary"
            onClick={async () => {
              await app.updateCatalog((v) => ({
                ...v,
                folderStyles: {
                  ...v.folderStyles,
                  [entry.path]: { color, symbol },
                },
              }));
              close();
            }}
          >
            {t("应用")}
          </Button>
        </>
      }
    >
      <Field label={t("文件夹颜色")}>
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
        />
      </Field>
      <Field label={t("符号 {0}", symbol)}>
        <SymbolGrid onChange={setSymbol} />
      </Field>
    </Modal>
  );
}
function Insights({ path }: { path: string }) {
  const app = useApp();
  const info = useAsync(
    () =>
      request<{ groups: { name: string; size: number; count: number }[] }>(
        "insights",
        { path },
      ),
    [path, app.refresh],
  );
  const total = info.data?.groups.reduce((n, g) => n + g.size, 0) || 0;
  const [screenshotFolder, setScreenshotFolder] = useState("");
  const [trashInstallers, setTrashInstallers] = useState(false);
  const [planning, setPlanning] = useState(false);
  async function tidy() {
    setPlanning(true);
    try {
      const p = await request<{ items: unknown[]; skipped: unknown[] }>(
        "tidy_plan",
        {
          path,
          screenshots: screenshotFolder || undefined,
          trash_installers: trashInstallers,
        },
      );
      app.showModal(<TidyReview plan={p} close={() => app.showModal(null)} />);
    } catch (e) {
      app.notify(String(e), true);
    } finally {
      setPlanning(false);
    }
  }
  return (
    <div className="insights">
      <div>
        <strong>{t("文件夹概览")}</strong>
        <span>{formatBytes(total)}</span>
        <div className="flex-1" />
        <Button disabled={planning} onClick={() => void tidy()}>
          <Layers size={13} />
          {t("整理文件…")}
        </Button>
      </div>
      <div className="insights-options">
        <Button
          title={screenshotFolder || undefined}
          onClick={async () => {
            const selected = await app.pickFiles({
              directories: true,
              initial: path,
            });
            if (selected[0]) setScreenshotFolder(selected[0]);
          }}
        >
          {t("截图去向")}
          {screenshotFolder ? ` · ${basename(screenshotFolder)}` : "…"}
        </Button>
        {screenshotFolder && (
          <IconButton
            label={t("恢复自动")}
            onClick={() => setScreenshotFolder("")}
          >
            <RotateCcw size={13} />
          </IconButton>
        )}
        <label>
          <input
            type="checkbox"
            checked={trashInstallers}
            onChange={(e) => setTrashInstallers(e.target.checked)}
          />
          {t("安装包移到回收站")}
        </label>
      </div>
      <div className="storage-bar">
        {info.data?.groups.map((g, i) => (
          <span
            key={g.name}
            title={`${g.name} · ${formatBytes(g.size)}`}
            style={{
              width: `${total ? (g.size / total) * 100 : 0}%`,
              background: [
                "#9b83ed",
                "#689ee5",
                "#dc8ca8",
                "#69bba5",
                "#dcac63",
                "#8d93b0",
              ][i % 6],
            }}
          />
        ))}
      </div>
      <div className="insight-legend">
        {info.data?.groups.map((g) => (
          <span key={g.name}>
            {g.name}
            <b>{g.count}</b>
          </span>
        ))}
      </div>
      <div className="insights-options">
        {(
          [
            ["duplicates", "找重复文件"],
            ["old", "检查旧下载"],
            ["installers", "检查安装包"],
          ] as const
        ).map(([category, label]) => (
          <Button
            key={category}
            onClick={() =>
              app.navigate({ type: "clean", scope: path, category })
            }
          >
            {t(label)}
          </Button>
        ))}
      </div>
    </div>
  );
}
function TidyReview({
  plan,
  close,
}: {
  plan: { items: any[]; skipped: unknown[] };
  close: () => void;
}) {
  const app = useApp();
  const [items, setItems] = useState(plan.items);
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      title={t("整理文件")}
      wide
      onClose={close}
      footer={
        <>
          <span className="muted flex-1">
            {t("跳过 ")}
            {plan.skipped.length}
            {t(" 个最近修改或下载中的文件")}
          </span>
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={busy || !items.some((v) => v.selected)}
            onClick={async () => {
              setBusy(true);
              try {
                await request("job_start", {
                  op: "tidy_apply",
                  args: { plan: { ...plan, items } },
                });
                close();
              } catch (e) {
                app.notify(String(e), true);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("开始整理")}
          </Button>
        </>
      }
    >
      <p className="muted">{t("按种类移动到子文件夹。完成后可以整体撤销。")}</p>
      <div className="insights-options">
        {[...new Set(items.map((item) => String(item.category)))].map(
          (category) => {
            const all = items
              .filter((item) => item.category === category)
              .every((item) => item.selected);
            return (
              <label key={category}>
                <input
                  type="checkbox"
                  checked={all}
                  onChange={(event) =>
                    setItems((old) =>
                      old.map((item) =>
                        item.category === category
                          ? { ...item, selected: event.target.checked }
                          : item,
                      ),
                    )
                  }
                />
                {t(category)}
              </label>
            );
          },
        )}
      </div>
      <div className="review-items">
        {items.map((v, i) => (
          <label key={v.entry.path}>
            <input
              type="checkbox"
              checked={v.selected}
              onChange={(e) =>
                setItems(
                  items.map((x, k) =>
                    k === i ? { ...x, selected: e.target.checked } : x,
                  ),
                )
              }
            />
            <FileIcon entry={v.entry} />
            <span>{v.entry.name}</span>
            <ChevronRight size={14} />
            <Badge>
              {v.action === "trash" ? t("移到回收站") : t(v.category)}
            </Badge>
          </label>
        ))}
      </div>
    </Modal>
  );
}

function Share({ entry, close }: { entry: Entry; close: () => void }) {
  const app = useApp();
  const load = useAsync(
    () => desktop<{ id: string; name: string }[]>("share_devices"),
    [],
  );
  return (
    <Modal title={t("分享文件")} onClose={close}>
      <p className="muted">{entry.name}</p>
      <h4>KDE Connect</h4>
      {load.loading ? (
        <Loading />
      ) : load.error ? (
        <p className="inline-warning">{load.error}</p>
      ) : load.data?.length ? (
        load.data.map((device) => (
          <Button
            key={device.id}
            onClick={async () => {
              try {
                await desktop("share", { path: entry.path, device: device.id });
                app.notify(t("文件已交给 KDE Connect"));
                close();
              } catch (e) {
                app.notify(String(e), true);
              }
            }}
          >
            {device.name || device.id}
          </Button>
        ))
      ) : (
        <p className="muted">
          {t("没有可用的已配对设备。请先在 KDE Connect 中配对。")}
        </p>
      )}
      <Button
        onClick={() => {
          void desktop("email", { path: entry.path }).catch((e) =>
            app.notify(String(e), true),
          );
          close();
        }}
      >
        {t("在邮件应用中添加附件")}
      </Button>
    </Modal>
  );
}
