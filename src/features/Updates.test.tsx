// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import { UpdatePanel } from "./Updates";
import { desktop } from "../api";
import {
  appContext,
  boot,
  button,
  click,
  installViewDOM,
  mount,
  providers,
  settle,
} from "../test/view-ui";
import { ViewMemory } from "../lib/view-memory";

vi.mock("../api", () => ({
  native: true,
  desktop: vi.fn(),
  request: vi.fn(),
  errorText: String,
}));
const progress = { fn: null as null | ((e: { payload: unknown }) => void) };
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name, fn) => {
    progress.fn = fn;
    return () => {
      progress.fn = null;
    };
  }),
}));
const rpc = vi.mocked(desktop);
beforeEach(() => {
  installViewDOM();
  rpc.mockReset();
  rpc.mockImplementation(async (name) => {
    if (name === "update_status")
      return { mode: "user", installed: false } as any;
    if (name === "update_check")
      return {
        mode: "user",
        update: {
          version: "0.2.4",
          notes: "Signed release",
          date: "2026-11-14",
        },
      } as any;
    return { ok: true } as any;
  });
});

it("checks on demand and shows the current version or a real newer release", async () => {
  const app = appContext({ boot: { ...boot, version: "0.2.3" } });
  const ui = await mount(providers(new ViewMemory(), <UpdatePanel />, app));
  await settle();
  expect(ui.container.textContent).toContain("0.2.3");
  expect(rpc.mock.calls.some(([a]) => a === "update_check")).toBe(false);
  await click(button(ui.container, "检查更新"));
  await settle();
  expect(ui.container.textContent).toContain("0.2.4");
  expect(ui.container.textContent).toContain("Signed release");
  expect(button(ui.container, "下载并安装")).toBeDefined();
  rpc.mockImplementation(async (name) =>
    name === "update_check"
      ? ({ mode: "user", update: null } as any)
      : ({ mode: "user" } as any),
  );
  await click(button(ui.container, "重新检查"));
  await settle();
  expect(ui.container.textContent).toContain("已是最新版本");
});

it("keeps installation explicit, displays progress and waits for an explicit restart", async () => {
  const app = appContext({
    confirm: vi.fn(async () => true),
    restartForUpdate: vi.fn(async () => {}),
  });
  const ui = await mount(providers(new ViewMemory(), <UpdatePanel />, app));
  await settle();
  await click(button(ui.container, "检查更新"));
  await settle();
  let finish!: () => void;
  rpc.mockImplementation(async (name) => {
    if (name === "update_install") {
      await new Promise<void>((r) => {
        finish = r;
      });
      return { ok: true } as any;
    }
    return { mode: "user" } as any;
  });
  await click(button(ui.container, "下载并安装"));
  await settle();
  await act(async () =>
    progress.fn?.({
      payload: { phase: "downloading", downloaded: 50, total: 100 },
    }),
  );
  expect(ui.container.textContent).toContain("50%");
  await act(async () => finish());
  await settle();
  expect(ui.container.textContent).toContain("重启后生效");
  expect(app.restartForUpdate).not.toHaveBeenCalled();
  await click(button(ui.container, "立即重启"));
  await settle();
  expect(app.restartForUpdate).toHaveBeenCalledOnce();
});

it("preserves the installed version on a failed download and offers retry", async () => {
  const app = appContext({ confirm: vi.fn(async () => true) });
  const ui = await mount(providers(new ViewMemory(), <UpdatePanel />, app));
  await settle();
  await click(button(ui.container, "检查更新"));
  await settle();
  rpc.mockImplementation(async (name) => {
    if (name === "update_install")
      throw new Error("Signature verification failed");
    return {} as any;
  });
  await click(button(ui.container, "下载并安装"));
  await settle();
  expect(ui.container.textContent).toContain("Signature verification failed");
  expect(ui.container.textContent).not.toContain("重启后生效");
  expect(button(ui.container, "下载并安装").disabled).toBe(false);
});

it("does not start an install with active file jobs or after a declined confirmation", async () => {
  const app = appContext({ confirm: vi.fn(async () => false) });
  const ui = await mount(providers(new ViewMemory(), <UpdatePanel />, app));
  await settle();
  await click(button(ui.container, "检查更新"));
  await settle();
  await click(button(ui.container, "下载并安装"));
  await settle();
  expect(rpc.mock.calls.some(([a]) => a === "update_install")).toBe(false);
  app.confirm = vi.fn(async () => true);
  app.jobs = [{ id: "job", status: "running" } as any];
  await ui.render(providers(new ViewMemory(), <UpdatePanel />, app));
  await settle();
  await click(button(ui.container, "下载并安装"));
  await settle();
  expect(rpc.mock.calls.some(([a]) => a === "update_install")).toBe(false);
  expect(ui.container.textContent).toContain("后台任务");
});

it("offers the official release for unmanaged builds and keeps network failures visible", async () => {
  rpc.mockImplementation(async (name) => {
    if (name === "update_status")
      return { mode: "manual", installed: false } as any;
    if (name === "update_check") throw new Error("Network unavailable");
    return {} as any;
  });
  const ui = await mount(
    providers(new ViewMemory(), <UpdatePanel />, appContext()),
  );
  await settle();
  await click(button(ui.container, "检查更新"));
  await settle();
  expect(ui.container.textContent).toContain("Network unavailable");
  await click(button(ui.container, "GitHub Releases"));
  await settle();
  expect(
    rpc.mock.calls.some(
      ([a, args]) =>
        a === "open_url" &&
        args?.url === "https://github.com/dBevil-z/FileManager/releases/latest",
    ),
  ).toBe(true);
});
