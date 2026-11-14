// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import { request } from "./api";
import {
  boot,
  button,
  click,
  input,
  installViewDOM,
  mount,
  settle,
  cleanupUI,
} from "./test/view-ui";
import type { Bootstrap, Entry } from "./types";

vi.mock("./api", () => ({
  native: false,
  request: vi.fn(),
  desktop: vi.fn(async () => []),
  errorText: String,
  mediaUrl: () => "",
}));
const rpc = vi.mocked(request);
let data: Bootstrap;
let session: Record<string, unknown>;
const download = "/home/demo/Downloads";
function entry(path: string): Entry {
  const markdown = path.endsWith(".md");
  return {
    path,
    name: path.split("/").pop()!,
    kind: markdown ? "markdown" : "folder",
    extension: markdown ? "md" : "",
    is_dir: !markdown,
    is_symlink: false,
    target_is_dir: false,
    hidden: false,
    size: 0,
    modified: null,
    created: null,
    accessed: null,
    inode: 1,
    device: 1,
    writable: true,
  };
}
beforeEach(() => {
  installViewDOM();
  localStorage.clear();
  session = {};
  data = JSON.parse(JSON.stringify(boot));
  data.catalog.data!.workspaces = [
    {
      id: "design",
      name: "Design project",
      description: "",
      color: "#9c7fea",
      paths: [],
      links: [],
      pinned: [],
    },
  ];
  rpc.mockReset();
  rpc.mockImplementation(async (op, args: any = {}) => {
    if (op === "bootstrap") return data as any;
    if (op === "plugins_list") return { plugins: [] } as any;
    if (op === "jobs" || op === "mounts") return [] as any;
    if (op === "git_status") return { repository: false } as any;
    if (op === "catalog_save") {
      if (args.namespace === "session")
        session = JSON.parse(
          JSON.stringify(args.data, (_key, value) =>
            value && typeof value === "object" && !Array.isArray(value)
              ? Object.fromEntries(
                  Object.entries(value).sort(([a], [b]) =>
                    a < b ? -1 : a > b ? 1 : 0,
                  ),
                )
              : value,
          ),
        );
      return { data: args.data, revision: 1 } as any;
    }
    if (op === "list_directory") {
      const names =
        args.path === download
          ? ["projects"]
          : args.path === download + "/projects"
            ? ["assets"]
            : args.path === download + "/projects/assets"
              ? ["photos", "notes.md"]
              : ["notes"];
      const entries = names.map((name) => entry(args.path + "/" + name));
      return {
        path: args.path,
        display_path: args.path,
        parent: args.path.slice(0, args.path.lastIndexOf("/")),
        entries,
        total: entries.length,
        writable: true,
        warnings: [],
      } as any;
    }
    if (op === "read_text")
      return { text: "# Original", revision: "one", line_ending: "LF" } as any;
    if (op === "preview")
      return { kind: "folder", metadata: {}, children: [] } as any;
    if (op === "folder_size")
      return { size: 0, files: 0, folders: 0, truncated: false } as any;
    if (op === "cache_stats") return { bytes: 0, items: 0 } as any;
    if (op === "resource_usage")
      return { pss: 0, rss: 0, processes: 0, rows: [] } as any;
    return {} as any;
  });
});
afterEach(async () => {
  await cleanupUI();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});
async function deep(container: HTMLElement) {
  await click(
    container.querySelector(`[data-path="${download}/projects"]`),
    "dblclick",
  );
  await click(
    container.querySelector(`[data-path="${download}/projects/assets"]`),
    "dblclick",
  );
}
const lastDirectory = () =>
  rpc.mock.calls.filter(([op]) => op === "list_directory").at(-1)?.[1] as {
    path: string;
  };

it("resumes nested local folders, visible filters, selection, scroll and session after switching menus", async () => {
  const ui = await mount(<App />);
  await deep(ui.container);
  await click(button(ui.container, "筛选文件 (Ctrl F)"));
  await input(
    ui.container.querySelector<HTMLInputElement>(".browser-filter input")!,
    "photo",
  );
  await click(ui.container.querySelector('[data-path$="/photos"]'));
  await act(async () => {
    const node = ui.container.querySelector<HTMLElement>(".file-scroll")!;
    node.scrollTop = 480;
    node.dispatchEvent(new Event("scroll"));
  });
  await click(button(ui.container.querySelector(".sidebar")!, "文档"));
  expect(lastDirectory().path).toBe("/home/demo/Documents");
  await click(button(ui.container.querySelector(".sidebar")!, "下载"));
  expect(lastDirectory().path).toBe(download + "/projects/assets");
  expect(
    ui.container.querySelector<HTMLInputElement>(".browser-filter input")
      ?.value,
  ).toBe("photo");
  expect(
    ui.container
      .querySelector('[data-path$="/photos"]')
      ?.getAttribute("aria-selected"),
  ).toBe("true");
  expect(
    ui.container.querySelector<HTMLElement>(".file-scroll")?.scrollTop,
  ).toBe(480);
  expect(
    ui.container.querySelector(".sidebar-item.selected")?.textContent,
  ).toBe("下载");
  expect(session.page).toEqual({
    type: "folder",
    path: download + "/projects/assets",
  });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 750));
  });
  expect(session.page).toEqual({
    type: "folder",
    path: download + "/projects/assets",
  });
  expect(session.viewState).toBeDefined();
  await ui.unmount();
  data.session.data = session;
  const restored = await mount(<App />);
  expect(lastDirectory().path).toBe(download + "/projects/assets");
  expect(
    restored.container.querySelector<HTMLInputElement>(".browser-filter input")
      ?.value,
  ).toBe("photo");
  expect(
    restored.container.querySelector<HTMLElement>(".file-scroll")?.scrollTop,
  ).toBe(480);
  expect(
    restored.container.querySelector(".sidebar-item.selected")?.textContent,
  ).toBe("下载");
  await restored.unmount();
});

it("resumes a workspace detail page and preserves menu provenance through back/forward", async () => {
  const ui = await mount(<App />);
  await click(button(ui.container.querySelector(".sidebar")!, "所有工作空间"));
  await click(ui.container.querySelector(".workspace-cover"));
  expect(
    ui.container.querySelector(".workspace-detail-cover")?.textContent,
  ).toContain("Design project");
  await click(button(ui.container.querySelector(".sidebar")!, "文档"));
  await click(button(ui.container, "后退"));
  expect(ui.container.querySelector(".workspace-detail-cover")).not.toBeNull();
  expect(
    ui.container.querySelector(".sidebar-item.selected")?.textContent,
  ).toContain("所有工作空间");
  await click(button(ui.container, "前进"));
  expect(lastDirectory().path).toBe("/home/demo/Documents");
  await click(button(ui.container.querySelector(".sidebar")!, "所有工作空间"));
  expect(ui.container.querySelector(".workspace-detail-cover")).not.toBeNull();
  await act(async () => {
    const node = ui.container.querySelector<HTMLElement>(".feature-page")!;
    node.scrollTop = 310;
    node.dispatchEvent(new Event("scroll"));
  });
  await click(button(ui.container.querySelector(".sidebar")!, "文档"));
  await click(button(ui.container.querySelector(".sidebar")!, "所有工作空间"));
  expect(
    ui.container.querySelector<HTMLElement>(".feature-page")?.scrollTop,
  ).toBe(310);
  await ui.unmount();
  data.session.data = session;
  const restored = await mount(<App />);
  expect(
    restored.container.querySelector(".workspace-detail-cover"),
  ).not.toBeNull();
  expect(
    restored.container.querySelector<HTMLElement>(".feature-page")?.scrollTop,
  ).toBe(310);
  expect(
    restored.container.querySelector(".sidebar-item.selected")?.textContent,
  ).toContain("所有工作空间");
  await restored.unmount();
});

it("keeps the unsaved Markdown guard and returns to its parent directory", async () => {
  const ui = await mount(<App />);
  await deep(ui.container);
  await click(
    ui.container.querySelector('[data-path$="/notes.md"]'),
    "dblclick",
  );
  await input(ui.container.querySelector("textarea")!, "# Unsaved test draft");
  await click(button(ui.container.querySelector(".sidebar")!, "文档"));
  expect(ui.container.textContent).toContain("文稿尚未保存");
  await click(button(document.body, "取消"));
  expect(ui.container.querySelector("textarea")?.value).toBe(
    "# Unsaved test draft",
  );
  await click(button(ui.container.querySelector(".sidebar")!, "文档"));
  await click(button(document.body, "确认"));
  expect(lastDirectory().path).toBe("/home/demo/Documents");
  await click(button(ui.container.querySelector(".sidebar")!, "下载"));
  expect(lastDirectory().path).toBe(download + "/projects/assets");
  expect(ui.container.querySelector("textarea")).toBeNull();
  await settle();
  await ui.unmount();
});

it("resets menu positions and honors disabled navigation memory", async () => {
  const ui = await mount(<App />);
  await deep(ui.container);
  await click(button(ui.container.querySelector(".sidebar")!, "设置"));
  await click(button(ui.container, "重置位置"));
  await click(button(ui.container.querySelector(".sidebar")!, "下载"));
  expect(lastDirectory().path).toBe(download);
  await deep(ui.container);
  await click(button(ui.container.querySelector(".sidebar")!, "设置"));
  await click(
    ui.container.querySelector('[role="switch"][aria-label="记住浏览位置"]'),
  );
  await click(button(ui.container.querySelector(".sidebar")!, "下载"));
  expect(lastDirectory().path).toBe(download);
  await deep(ui.container);
  await click(button(ui.container.querySelector(".sidebar")!, "文档"));
  await click(button(ui.container.querySelector(".sidebar")!, "下载"));
  expect(lastDirectory().path).toBe(download);
  await ui.unmount();
  data.settings.data!.rememberNavigation = false;
  data.session.data = {
    page: { type: "folder", path: download + "/projects/assets" },
  };
  const restored = await mount(<App />);
  expect(lastDirectory().path).toBe(download);
  await restored.unmount();
});
