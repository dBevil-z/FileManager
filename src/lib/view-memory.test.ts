import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ViewMemory,
  isPage,
  menuForPage,
  pageKey,
  rememberMenu,
  resumeMenu,
  viewKey,
} from "./view-memory";
import type { Page } from "../types";

afterEach(() => vi.useRealTimers());
describe("menu navigation memory", () => {
  const downloads: Page = { type: "folder", path: "/home/demo/Downloads" };
  const documents: Page = { type: "folder", path: "/home/demo/Documents" };
  const roots = [downloads, documents, { type: "folder", path: "/" } as Page];
  it("uses stable route keys when the Rust backend sorts JSON object fields", () => {
    expect(pageKey(downloads)).toBe(
      pageKey({ path: downloads.path, type: "folder" }),
    );
    const first: Page = {
      type: "markdown",
      path: "/note.md",
      from: { type: "workspace", id: "all" },
    };
    const sorted: Page = {
      from: { id: "all", type: "workspace" },
      path: "/note.md",
      type: "markdown",
    };
    expect(pageKey(first)).toBe(pageKey(sorted));
  });
  it("preserves menu origin and restoration for native non-UTF-8 directory tokens", () => {
    const raw =
      Array.from(new TextEncoder().encode(downloads.path + "/"), (v) =>
        String.fromCharCode(v),
      ).join("") + "\xff";
    const path =
      "fileos:bytes:" +
      btoa(raw).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    const page: Page = { type: "folder", path };
    expect(isPage(page)).toBe(true);
    expect(menuForPage(page, roots, downloads)).toEqual(downloads);
    const memory = new ViewMemory();
    rememberMenu(memory, downloads, page);
    expect(resumeMenu(new ViewMemory(memory.snapshot()), downloads)).toEqual(
      page,
    );
    expect(isPage({ type: "folder", path: "fileos:bytes:invalid" })).toBe(
      false,
    );
  });
  it("returns each menu to its own descendant and keeps explicit roots available", () => {
    const memory = new ViewMemory();
    const deep: Page = {
      type: "folder",
      path: "/home/demo/Downloads/work/assets",
    };
    rememberMenu(memory, downloads, deep);
    rememberMenu(memory, documents, {
      type: "folder",
      path: "/home/demo/Documents/notes",
    });
    expect(resumeMenu(memory, downloads)).toEqual(deep);
    expect(menuForPage(deep, roots, downloads)).toEqual(downloads);
    expect(
      menuForPage(
        { type: "folder", path: "/home/demo/Downloads-other" },
        roots,
        downloads,
      ),
    ).toEqual({ type: "folder", path: "/" });
    const restored = new ViewMemory(
      JSON.parse(JSON.stringify(memory.snapshot())),
    );
    expect(resumeMenu(restored, downloads)).toEqual(deep);
    restored.clear();
    expect(resumeMenu(restored, downloads)).toEqual(downloads);
  });
  it("restores workspace detail without reopening an unsaved editor", () => {
    const memory = new ViewMemory();
    const root: Page = { type: "workspace", id: "all" };
    const detail: Page = { type: "workspace", id: "design" };
    rememberMenu(memory, root, {
      type: "markdown",
      path: "/notes.md",
      from: detail,
    });
    expect(resumeMenu(memory, root)).toEqual(detail);
    expect(menuForPage(detail, roots, root)).toEqual(root);
    expect(menuForPage(root, roots, detail)).toEqual(root);
  });
  it("falls back from malformed routes and separates ambiguous identifiers", () => {
    const memory = new ViewMemory();
    memory.set(viewKey("menu", pageKey(downloads)), {
      type: "folder",
      path: 3,
    });
    expect(resumeMenu(memory, downloads)).toEqual(downloads);
    memory.set(viewKey("menu", pageKey(downloads)), { type: "packages" });
    expect(resumeMenu(memory, downloads)).toEqual(downloads);
    expect(isPage({ type: "unknown" })).toBe(false);
    expect(viewKey("a:b", "c")).not.toBe(viewKey("a", "b:c"));
  });
});

describe("bounded UI snapshots", () => {
  it("keeps recently revisited controls when pruning old directory history", () => {
    const memory = new ViewMemory();
    for (let i = 0; i < 256; i++) memory.set(String(i), i);
    expect(memory.get("0")).toBe(0);
    memory.set("256", 256);
    expect(memory.get("0")).toBe(0);
    expect(memory.get("1")).toBeUndefined();
  });
  it("bounds history bytes and entries while retaining recent locations", () => {
    const memory = new ViewMemory();
    const menu: Page = { type: "folder", path: "/downloads" };
    rememberMenu(memory, menu, { type: "folder", path: "/downloads/project" });
    for (let i = 0; i < 600; i++)
      memory.set(viewKey("folder", i), "资料".repeat(1000));
    expect(memory.snapshot().entries.length).toBeLessThanOrEqual(256);
    expect(
      new TextEncoder().encode(JSON.stringify(memory.snapshot())).length,
    ).toBeLessThanOrEqual(256 * 1024);
    expect(memory.get(viewKey("folder", 599))).toBeDefined();
    expect(memory.get(viewKey("folder", 0))).toBeUndefined();
    expect(resumeMenu(memory, menu)).toEqual({
      type: "folder",
      path: "/downloads/project",
    });
  });
  it("omits runtime jobs, secrets, and oversized state from durable storage", () => {
    const memory = new ViewMemory();
    memory.set("remote:path", "work/");
    memory.set("transient:job", { id: "job-1" });
    memory.set("accidental-form", { nested: { password: "sensitive" } });
    memory.set("large", "x".repeat(40_000));
    memory.set("optional-route", { type: "clean", scope: undefined });
    const snapshot = JSON.stringify(memory.snapshot());
    expect(snapshot).not.toContain("sensitive");
    expect(snapshot).not.toContain("job-1");
    expect(snapshot).not.toContain("accidental-form");
    expect(memory.get("optional-route")).toEqual({ type: "clean" });
    expect(new ViewMemory(JSON.parse(snapshot)).get("remote:path")).toBe(
      "work/",
    );
  });
  it("clears old selection rather than restoring a stale subset after a large selection", () => {
    const memory = new ViewMemory();
    memory.set("selection", ["old"]);
    memory.set(
      "selection",
      Array.from({ length: 1500 }, (_, i) => String(i)),
    );
    expect(memory.get("selection")).toBeUndefined();
  });
  it("disabled memory does not restore or record navigation", () => {
    const memory = new ViewMemory(
      { version: 1, entries: [["old", "saved"]] },
      false,
    );
    memory.set("new", "value");
    expect(memory.snapshot().entries).toEqual([]);
    memory.setEnabled(true);
    memory.set("new", "value");
    memory.setEnabled(false);
    expect(memory.get("new")).toBeUndefined();
  });
  it("evicts and closes cached cursors, never serializing them", () => {
    vi.useFakeTimers();
    const memory = new ViewMemory();
    const dispose = Array.from({ length: 5 }, () => vi.fn());
    for (let i = 0; i < 5; i++)
      memory.cacheRuntime(String(i), { cursor: `token-${i}` }, dispose[i]);
    expect(dispose[0]).toHaveBeenCalledOnce();
    expect(memory.takeRuntime("0")).toBeUndefined();
    expect(memory.takeRuntime("1")).toEqual({ cursor: "token-1" });
    expect(dispose[1]).not.toHaveBeenCalled();
    expect(JSON.stringify(memory.snapshot())).not.toContain("token");
    vi.advanceTimersByTime(240_001);
    for (const i of [2, 3, 4]) expect(dispose[i]).toHaveBeenCalledOnce();
    memory.dispose();
  });
});
