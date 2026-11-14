import type { Page } from "../types";

export type Json =
  null | boolean | number | string | Json[] | { [key: string]: Json };
export type ViewSnapshot = { version: 1; entries: [string, Json][] };
const MAX_ENTRIES = 256;
const MAX_BYTES = 256 * 1024 - 32; // Include the version/envelope in the budget.
const MAX_VALUE_BYTES = 32 * 1024;
const RUNTIME_BYTES = 2 * 1024 * 1024;
const encoder = new TextEncoder();
const forbidden =
  /^(?:__proto__|constructor|prototype|password|secret|token|passphrase|credentials|content|results?)$/i;

function safe(value: unknown, depth = 0): value is Json {
  if (depth > 8) return false;
  if (value === null || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (typeof value === "string") return value.length <= 8192;
  if (Array.isArray(value))
    return value.length <= 1000 && value.every((v) => safe(v, depth + 1));
  return (
    typeof value === "object" &&
    Object.getPrototypeOf(value) === Object.prototype &&
    Object.entries(value).every(
      ([k, v]) => !forbidden.test(k) && safe(v, depth + 1),
    )
  );
}

/** Small UI snapshots only. Runtime directory cursors are never serialized. */
export class ViewMemory {
  private values = new Map<
    string,
    { value: Json; bytes: number; text: string }
  >();
  private bytes = 0;
  private listeners = new Set<() => void>();
  private runtime = new Map<
    string,
    { value: unknown; bytes: number; expires: number; dispose: () => void }
  >();
  private timer: ReturnType<typeof setTimeout> | undefined;
  enabled: boolean;
  constructor(snapshot?: unknown, enabled = true) {
    this.enabled = enabled;
    if (
      enabled &&
      snapshot &&
      typeof snapshot === "object" &&
      (snapshot as ViewSnapshot).version === 1 &&
      Array.isArray((snapshot as ViewSnapshot).entries)
    ) {
      for (const item of (snapshot as ViewSnapshot).entries.slice(-MAX_ENTRIES))
        if (
          Array.isArray(item) &&
          item.length === 2 &&
          typeof item[0] === "string"
        )
          this.set(item[0], item[1]);
    }
  }
  get<T>(key: string): T | undefined {
    if (!this.enabled) return undefined;
    const entry = this.values.get(key);
    if (entry) {
      this.values.delete(key);
      this.values.set(key, entry);
    }
    return entry?.value as T | undefined;
  }
  set(key: string, value: unknown) {
    if (!this.enabled || key.length > 8192) return;
    let text: string;
    let normalized: unknown;
    try {
      text = JSON.stringify(value);
      normalized = JSON.parse(text);
    } catch {
      return;
    }
    const bytes = encoder.encode(JSON.stringify([key, normalized])).length + 1;
    if (!safe(normalized) || bytes > MAX_VALUE_BYTES) {
      const old = this.values.get(key);
      if (old) {
        this.values.delete(key);
        this.bytes -= old.bytes;
        this.emit();
      }
      return;
    }
    if (this.values.get(key)?.text === text) return;
    const previous = this.values.get(key);
    if (previous) this.bytes -= previous.bytes;
    this.values.delete(key);
    this.values.set(key, { value: normalized as Json, bytes, text });
    this.bytes += bytes;
    while (this.values.size > MAX_ENTRIES || this.bytes > MAX_BYTES) {
      // Reserve menu locations ahead of individual controls when pruning history.
      const keys = [...this.values.keys()];
      const oldest =
        keys.find((k) => k !== "menu:active" && !k.startsWith('["menu",')) ||
        keys[0];
      this.bytes -= this.values.get(oldest)!.bytes;
      this.values.delete(oldest);
    }
    this.emit();
  }
  snapshot(): ViewSnapshot {
    return {
      version: 1,
      entries: [...this.values]
        .filter(([key]) => !key.startsWith("transient:"))
        .map(([key, v]) => [key, v.value]),
    };
  }
  subscribe(listener: () => void) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
  private emit() {
    for (const listener of this.listeners) listener();
  }
  setEnabled(enabled: boolean) {
    if (this.enabled === enabled) return;
    this.enabled = enabled;
    this.clear();
  }
  clear() {
    this.values.clear();
    this.bytes = 0;
    this.dispose();
    this.emit();
  }
  private pruneRuntime() {
    for (const [key, entry] of this.runtime)
      if (entry.expires <= Date.now()) {
        this.runtime.delete(key);
        entry.dispose();
      }
  }
  private schedule() {
    clearTimeout(this.timer);
    if (this.runtime.size)
      this.timer = setTimeout(
        () => {
          this.pruneRuntime();
          this.schedule();
        },
        Math.max(
          1,
          Math.min(...[...this.runtime.values()].map((v) => v.expires)) -
            Date.now(),
        ),
      );
  }
  takeRuntime<T>(key: string): T | undefined {
    this.pruneRuntime();
    const entry = this.runtime.get(key);
    this.runtime.delete(key);
    this.schedule();
    return entry?.value as T | undefined;
  }
  cacheRuntime(
    key: string,
    value: unknown,
    dispose: () => void,
    ttl = 240_000,
  ) {
    this.pruneRuntime();
    const bytes = encoder.encode(JSON.stringify(value)).length;
    if (!this.enabled || bytes > RUNTIME_BYTES || ttl <= 0) {
      dispose();
      return;
    }
    const old = this.runtime.get(key);
    if (old) {
      this.runtime.delete(key);
      old.dispose();
    }
    this.runtime.set(key, {
      value,
      bytes,
      dispose,
      expires: Date.now() + Math.min(ttl, 240_000),
    });
    while (
      this.runtime.size > 4 ||
      [...this.runtime.values()].reduce((n, v) => n + v.bytes, 0) >
        RUNTIME_BYTES
    ) {
      const oldest = this.runtime.keys().next().value!;
      const entry = this.runtime.get(oldest)!;
      this.runtime.delete(oldest);
      entry.dispose();
    }
    this.schedule();
  }
  dispose() {
    clearTimeout(this.timer);
    for (const entry of this.runtime.values()) entry.dispose();
    this.runtime.clear();
  }
}

export const viewKey = (...parts: (string | number)[]) => JSON.stringify(parts);
// Compare Linux path bytes without converting invalid UTF-8 names to replacement characters.
function pathBytes(path: string): string | null {
  try {
    if (!path.startsWith("fileos:bytes:"))
      return Array.from(encoder.encode(path), (v) =>
        String.fromCharCode(v),
      ).join("");
    const token = path
      .slice("fileos:bytes:".length)
      .replace(/-/g, "+")
      .replace(/_/g, "/");
    return atob(token.padEnd(Math.ceil(token.length / 4) * 4, "="));
  } catch {
    return null;
  }
}
function withinFolder(path: string, root: string) {
  const child = pathBytes(path),
    parent = pathBytes(root);
  return Boolean(
    child?.startsWith("/") &&
    parent?.startsWith("/") &&
    (parent === "/" ||
      child === parent ||
      child.startsWith(parent.replace(/\/$/, "") + "/")),
  );
}
export function isPage(value: unknown, depth = 0): value is Page {
  if (!value || typeof value !== "object" || depth > 3) return false;
  const v = value as Record<string, unknown>;
  const text = (key: string) =>
    typeof v[key] === "string" && (v[key] as string).length <= 4096;
  switch (v.type) {
    case "folder":
      return (
        text("path") &&
        !String(v.path).includes("\0") &&
        Boolean(pathBytes(v.path as string)?.startsWith("/"))
      );
    case "workspace":
      return text("id");
    case "bookmarks":
      return (
        (v.id === undefined || text("id")) &&
        (v.tag === undefined || text("tag")) &&
        (v.favourites === undefined || typeof v.favourites === "boolean")
      );
    case "markdown":
      return (
        text("path") && (v.from === undefined || isPage(v.from, depth + 1))
      );
    case "clean":
      return (
        (v.scope === undefined || text("scope")) &&
        (v.category === undefined || text("category"))
      );
    default:
      return [
        "packages",
        "plugins",
        "remote",
        "about",
        "agents",
        "system",
        "settings",
        "trash",
      ].includes(String(v.type));
  }
}
export function pageKey(page: Page): string {
  return JSON.stringify(
    Object.fromEntries(
      Object.entries(page)
        .filter(([, value]) => value !== undefined)
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([key, value]) => [
          key,
          key === "from" && page.type === "markdown" && page.from
            ? JSON.parse(pageKey(page.from))
            : value,
        ]),
    ),
  );
}
export function menuForPage(page: Page, roots: Page[], current?: Page): Page {
  if (page.type === "markdown")
    return page.from
      ? menuForPage(page.from, roots, current)
      : {
          type: "folder",
          path: page.path.slice(0, page.path.lastIndexOf("/")) || "/",
        };
  if (page.type === "workspace" && page.id === "all") return page;
  if (current?.type === page.type && page.type !== "folder") return current;
  if (page.type === "folder") {
    const within = (root: Page) =>
      root.type === "folder" && withinFolder(page.path, root.path);
    if (current?.type === "folder" && within(current)) return current;
    return (
      roots
        .filter(within)
        .sort(
          (a, b) =>
            (pathBytes((b as { path: string }).path)?.length || 0) -
            (pathBytes((a as { path: string }).path)?.length || 0),
        )[0] || page
    );
  }
  if (page.type === "workspace") return { type: "workspace", id: "all" };
  if (page.type === "bookmarks") return { type: "bookmarks" };
  if (page.type === "clean") return { type: "clean" };
  return page;
}
export function rememberMenu(memory: ViewMemory, menu: Page, page: Page) {
  memory.set("menu:active", menu);
  const location = page.type === "markdown" ? page.from : page;
  if (location) memory.set(viewKey("menu", pageKey(menu)), location);
}
export function resumeMenu(memory: ViewMemory, menu: Page): Page {
  const page = memory.get<unknown>(viewKey("menu", pageKey(menu)));
  if (!isPage(page) || page.type !== menu.type) return menu;
  if (
    page.type === "folder" &&
    menu.type === "folder" &&
    !withinFolder(page.path, menu.path)
  )
    return menu;
  if (
    page.type === "workspace" &&
    menu.type === "workspace" &&
    menu.id !== "all" &&
    page.id !== menu.id
  )
    return menu;
  return page;
}
