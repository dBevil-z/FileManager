// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import { desktop, request } from "./api";
import {
  boot,
  button,
  click,
  installViewDOM,
  mount,
  settle,
} from "./test/view-ui";
import { useApp } from "./context";

vi.mock("./api", () => ({
  native: false,
  request: vi.fn(),
  desktop: vi.fn(async () => []),
  errorText: String,
  mediaUrl: () => "",
}));
vi.mock("./features/Updates", () => ({
  UpdatePanel: () => {
    const app = useApp();
    return (
      <button
        onClick={() =>
          void app.restartForUpdate().catch((e) => app.notify(String(e), true))
        }
      >
        Test restart
      </button>
    );
  },
}));
const rpc = vi.mocked(request);
let hold: null | (() => Promise<unknown>);
let activeJob: boolean;
beforeEach(() => {
  installViewDOM();
  hold = null;
  activeJob = false;
  rpc.mockReset();
  vi.mocked(desktop).mockClear();
  rpc.mockImplementation(async (op, args: any = {}) => {
    if (op === "bootstrap")
      return {
        ...boot,
        session: { revision: 0, data: { page: { type: "about" } } },
      } as any;
    if (op === "plugins_list") return { plugins: [] } as any;
    if (op === "jobs")
      return (activeJob ? [{ id: "active", status: "running" }] : []) as any;
    if (op === "catalog_save") {
      if (args.namespace === "session" && hold) await hold();
      return { revision: 1, data: args.data } as any;
    }
    if (op === "resource_usage") return { rss: 0, pss: 0, processes: 1 } as any;
    if (op === "mounts") return [] as any;
    return {} as any;
  });
});

it("persists the current browsing session before invoking native restart", async () => {
  const ui = await mount(<App />);
  await settle();
  let release!: () => void;
  hold = () =>
    new Promise<void>((resolve) => {
      release = resolve;
    });
  await click(button(ui.container, "Test restart"));
  await settle();
  expect(
    vi.mocked(desktop).mock.calls.some(([a]) => a === "update_restart"),
  ).toBe(false);
  const saves = rpc.mock.calls.filter(
    ([a, v]) => a === "catalog_save" && v?.namespace === "session",
  );
  expect((saves.at(-1)?.[1]?.data as any).page).toEqual({ type: "about" });
  await act(async () => release());
  await settle();
  expect(
    vi.mocked(desktop).mock.calls.some(([a]) => a === "update_restart"),
  ).toBe(true);
});

it("does not restart after a failed session write or with an active backend job", async () => {
  const ui = await mount(<App />);
  await settle();
  hold = async () => {
    throw new Error("session disk failure");
  };
  await click(button(ui.container, "Test restart"));
  await settle();
  expect(ui.container.textContent).toContain("session disk failure");
  expect(
    vi.mocked(desktop).mock.calls.some(([a]) => a === "update_restart"),
  ).toBe(false);
  hold = null;
  activeJob = true;
  await click(button(ui.container, "Test restart"));
  await settle();
  expect(ui.container.textContent).toContain("后台任务");
  expect(
    vi.mocked(desktop).mock.calls.some(([a]) => a === "update_restart"),
  ).toBe(false);
});

it("respects the existing unsaved-change navigation guard before restarting", async () => {
  const ui = await mount(<App />);
  await settle();
  let proceed!: () => void;
  const guard = (event: Event) => {
    event.preventDefault();
    proceed = (event as CustomEvent).detail.proceed;
  };
  window.addEventListener("filemanager-before-navigate", guard);
  try {
    await click(button(ui.container, "Test restart"));
    await settle();
    expect(
      vi.mocked(desktop).mock.calls.some(([a]) => a === "update_restart"),
    ).toBe(false);
    await act(async () => proceed());
    await settle();
    expect(
      vi.mocked(desktop).mock.calls.some(([a]) => a === "update_restart"),
    ).toBe(true);
  } finally {
    window.removeEventListener("filemanager-before-navigate", guard);
  }
});
