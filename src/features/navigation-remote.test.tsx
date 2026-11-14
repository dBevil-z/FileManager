// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { RemoteStoragePage, type Listing, type Profile } from "./RemoteStorage";
import { request } from "../api";
import { ViewMemory } from "../lib/view-memory";
import {
  button,
  click,
  input,
  installViewDOM,
  mount,
  providers,
  cleanupUI,
  settle,
} from "../test/view-ui";

vi.mock("../api", () => ({
  native: false,
  request: vi.fn(),
  desktop: vi.fn(),
  errorText: String,
}));
const rpc = vi.mocked(request);
let profiles: Profile[];
let serial = 0;
const cursors = new Map<string, number>();
const list = (names: string[], path = "", dirs = false): Listing => ({
  entries: names.map((name) => ({
    name,
    path: path + name + (dirs ? "/" : ""),
    is_dir: dirs,
    size: dirs ? null : 12,
    modified: null,
  })),
  truncated: false,
  skipped: 0,
  next_cursor: null,
});

it("closes a late remote cursor when its page was left while loading", async () => {
  const memory = new ViewMemory();
  const original = rpc.getMockImplementation()!;
  let resolve!: (value: Listing) => void;
  rpc.mockImplementation(async (op, args: any) => {
    if (op === "remote_list" && args.path === "projects/deep/")
      return (await new Promise<Listing>((done) => {
        resolve = done;
      })) as any;
    return original(op, args);
  });
  const ui = await mount(providers(memory, <RemoteStoragePage />));
  await openDeep(ui.container);
  await ui.render(providers(memory, <div />));
  await act(async () => {
    resolve({
      ...list(["late.txt"], "projects/deep/"),
      next_cursor: "late-cursor",
    });
  });
  await settle();
  expect(rpc).toHaveBeenCalledWith("remote_list_close", {
    id: "alpha",
    cursor: "late-cursor",
  });
  expect(JSON.stringify(memory.snapshot())).not.toContain("late-cursor");
  await ui.unmount();
  memory.dispose();
});
beforeEach(() => {
  installViewDOM();
  serial = 0;
  cursors.clear();
  rpc.mockReset();
  profiles = ["alpha", "beta"].map((id) => ({
    id,
    revision: 1,
    remember: false,
    needs_secret: false,
    config: {
      label: id,
      endpoint: "http://localhost:9000",
      root: "",
      read_only: true,
      provider: { kind: "minio", bucket: "", region: "", user: "demo" },
    },
  }));
  rpc.mockImplementation(async (op, args: any = {}) => {
    if (op === "remote_profiles") return profiles as any;
    if (op === "remote_list_close") {
      cursors.delete(args.cursor);
      return null as any;
    }
    if (op !== "remote_list") throw new Error("Unexpected RPC: " + op);
    if (!args.bucket) return list(["assets", "documents"], "", true) as any;
    if (!args.path) return list(["projects"], "", true) as any;
    if (args.path === "projects/")
      return list(["deep"], args.path, true) as any;
    let page = 1;
    if (args.cursor) {
      if (!cursors.has(args.cursor)) throw new Error("Stale cursor was reused");
      page = cursors.get(args.cursor)!;
      cursors.delete(args.cursor);
    }
    const result = list([`chapter-${page}.txt`], args.path);
    if (page < 3) {
      result.next_cursor = `cursor-${++serial}`;
      cursors.set(result.next_cursor, page + 1);
    }
    return result as any;
  });
});
afterEach(async () => {
  await cleanupUI();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

async function openDeep(container: HTMLElement) {
  await click(container.querySelectorAll(".remote-connection")[0]);
  await click(button(container, "assets"));
  await click(button(container, "projects"));
  await click(button(container, "deep"));
}
const filter = (container: HTMLElement) =>
  container.querySelector<HTMLInputElement>(".remote-list-filter input")!;

it("returns to the same connection, Bucket, directory, filter, selection and page after a menu switch", async () => {
  const memory = new ViewMemory();
  const ui = await mount(providers(memory, <RemoteStoragePage />));
  await openDeep(ui.container);
  await input(filter(ui.container), "chapter");
  await click(button(ui.container, "下一页"));
  const checkbox = ui.container.querySelector<HTMLInputElement>(
    'input[type="checkbox"]',
  )!;
  await click(checkbox);
  expect(checkbox.checked).toBe(true);
  const requestsBefore = rpc.mock.calls.filter(
    ([op]) => op === "remote_list",
  ).length;
  await ui.render(providers(memory, <div>Other menu</div>));
  await ui.render(providers(memory, <RemoteStoragePage />));
  expect(
    ui.container.querySelector('.remote-connection[aria-pressed="true"]')
      ?.textContent,
  ).toContain("alpha");
  expect(
    ui.container.querySelector('.remote-breadcrumbs [aria-current="location"]')
      ?.textContent,
  ).toBe("deep");
  expect(ui.container.textContent).toContain("第 2 页");
  expect(ui.container.textContent).toContain("chapter-2.txt");
  expect(filter(ui.container).value).toBe("chapter");
  expect(
    ui.container.querySelector<HTMLInputElement>('input[type="checkbox"]')
      ?.checked,
  ).toBe(true);
  expect(rpc.mock.calls.filter(([op]) => op === "remote_list").length).toBe(
    requestsBefore,
  );
  await click(button(ui.container, "所有 Bucket"));
  await click(button(ui.container, "documents"));
  expect(
    ui.container.querySelector('.remote-breadcrumbs [aria-current="location"]')
      ?.textContent,
  ).toBe("起始目录");
  await click(ui.container.querySelectorAll(".remote-connection")[1]);
  expect(ui.container.querySelector(".remote-buckets-table")).not.toBeNull();
  await click(ui.container.querySelectorAll(".remote-connection")[0]);
  expect(
    ui.container.querySelector(".remote-bucket-nav strong")?.textContent,
  ).toBe("documents");
  await click(button(ui.container, "所有 Bucket"));
  await click(button(ui.container, "assets"));
  expect(
    ui.container.querySelector('.remote-breadcrumbs [aria-current="location"]')
      ?.textContent,
  ).toBe("deep");
  expect(ui.container.textContent).toContain("第 2 页");
  await ui.unmount();
  memory.dispose();
  expect(rpc.mock.calls.some(([op]) => op === "remote_list_close")).toBe(true);
});

it("replays fresh pages after restart without storing or reusing old server cursors", async () => {
  const memory = new ViewMemory();
  const ui = await mount(providers(memory, <RemoteStoragePage />));
  await openDeep(ui.container);
  await click(button(ui.container, "下一页"));
  await input(filter(ui.container), "chapter");
  const snapshot = JSON.parse(JSON.stringify(memory.snapshot()));
  expect(JSON.stringify(snapshot)).not.toContain("cursor-");
  await ui.unmount();
  memory.dispose();
  cursors.clear();
  rpc.mockClear();
  const restored = new ViewMemory(snapshot);
  const next = await mount(providers(restored, <RemoteStoragePage />));
  expect(next.container.textContent).toContain("第 2 页");
  expect(filter(next.container).value).toBe("chapter");
  const lists = rpc.mock.calls.filter(([op]) => op === "remote_list");
  expect(lists).toHaveLength(2);
  expect(lists[0][1]).toMatchObject({
    id: "alpha",
    bucket: "assets",
    path: "projects/deep/",
  });
  expect(lists[0][1]).not.toHaveProperty("cursor");
  expect(lists[1][1]).toHaveProperty("cursor");
  await next.unmount();
  restored.dispose();
});

it("keeps credentials required after restart and isolates edited connection revisions", async () => {
  const memory = new ViewMemory();
  const ui = await mount(providers(memory, <RemoteStoragePage />));
  await openDeep(ui.container);
  await ui.render(providers(memory, <div />));
  profiles = profiles.map((p) =>
    p.id === "alpha" ? { ...p, needs_secret: true } : p,
  );
  rpc.mockClear();
  await ui.render(providers(memory, <RemoteStoragePage />));
  expect(ui.container.textContent).toContain("此连接需要凭据");
  expect(rpc.mock.calls.some(([op]) => op === "remote_list")).toBe(false);
  await click(button(ui.container, "输入凭据"));
  await input(
    ui.container.querySelector<HTMLInputElement>('input[type="password"]')!,
    "not-a-real-secret",
  );
  expect(JSON.stringify(memory.snapshot())).not.toContain("not-a-real-secret");
  await ui.render(providers(memory, <div />));
  profiles = profiles.map((p) =>
    p.id === "alpha" ? { ...p, revision: 2, needs_secret: false } : p,
  );
  await ui.render(providers(memory, <RemoteStoragePage />));
  expect(ui.container.querySelector(".remote-buckets-table")).not.toBeNull();
  expect(ui.container.querySelector(".remote-breadcrumbs")).toBeNull();
  await ui.unmount();
  memory.dispose();
});
