// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Clean, Packages } from "./Tools";
import { request } from "../api";
import { ViewMemory } from "../lib/view-memory";
import {
  button,
  cleanupUI,
  click,
  input,
  installViewDOM,
  mount,
  providers,
} from "../test/view-ui";

vi.mock("../api", () => ({
  native: false,
  request: vi.fn(),
  desktop: vi.fn(),
  errorText: String,
}));
const rpc = vi.mocked(request);
beforeEach(() => {
  installViewDOM();
  rpc.mockReset();
});
afterEach(async () => {
  await cleanupUI();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

it("reattaches to the original scan without rescanning or retaining the full result in view memory", async () => {
  const memory = new ViewMemory();
  const result = {
    categories: [
      {
        id: "duplicates",
        name: "重复文件",
        items: Array.from({ length: 160 }, (_, i) => ({
          keeper: false,
          reason: "重复文件",
          entry: {
            path: `/scan/file-${i}.txt`,
            name: `file-${i}.txt`,
            kind: "text",
            size: 10,
          },
        })),
        reclaimable: 1600,
      },
    ],
  };
  rpc.mockImplementation(async (op) => {
    if (op === "system_info") return {} as any;
    if (op === "job_start") return { id: "scan-one" } as any;
    if (op === "jobs")
      return [{ id: "scan-one", status: "done", result }] as any;
    throw new Error("Unexpected RPC: " + op);
  });
  const ui = await mount(providers(memory, <Clean scope="/scan" />));
  await click(button(ui.container, "下一页"));
  await click(ui.container.querySelector("tbody tr"));
  expect(
    ui.container.querySelector<HTMLInputElement>('tbody input[type="checkbox"]')
      ?.checked,
  ).toBe(true);
  await ui.render(providers(memory, <div />));
  await ui.render(providers(memory, <Clean scope="/scan" />));
  expect(ui.container.querySelector("tbody tr")?.textContent).toContain(
    "file-150.txt",
  );
  expect(
    ui.container.querySelector<HTMLInputElement>('tbody input[type="checkbox"]')
      ?.checked,
  ).toBe(true);
  expect(rpc.mock.calls.filter(([op]) => op === "job_start")).toHaveLength(1);
  expect(rpc.mock.calls.filter(([op]) => op === "jobs")).toHaveLength(2);
  expect(JSON.stringify(memory.snapshot())).not.toContain("scan-one");
  expect(JSON.stringify(memory.snapshot())).not.toContain("file-150.txt");
  await ui.unmount();
  memory.dispose();
});

it("explains expired scan results and lets the user explicitly scan again", async () => {
  const memory = new ViewMemory();
  let expired = false;
  rpc.mockImplementation(async (op) => {
    if (op === "system_info") return {} as any;
    if (op === "job_start") return { id: "scan-one" } as any;
    if (op === "jobs")
      return [
        {
          id: "scan-one",
          status: "done",
          result_expired: expired,
          result: expired ? undefined : { categories: [] },
        },
      ] as any;
    throw new Error("Unexpected RPC: " + op);
  });
  const ui = await mount(providers(memory, <Clean scope="/scan" />));
  await ui.render(providers(memory, <div />));
  expired = true;
  await ui.render(providers(memory, <Clean scope="/scan" />));
  expect(ui.container.textContent).toContain("扫描结果已释放，请重新扫描");
  expect(rpc.mock.calls.filter(([op]) => op === "job_start")).toHaveLength(1);
  expect(button(ui.container, "开始扫描").disabled).toBe(false);
  await ui.unmount();
  memory.dispose();
});

it("keeps package pagination and selection on return, but resets pagination when filters change", async () => {
  const memory = new ViewMemory();
  rpc.mockResolvedValue({
    items: Array.from({ length: 205 }, (_, i) => ({
      id: String(i),
      name: `package-${String(i).padStart(3, "0")}`,
      description: "",
      manager: "apt",
      reason: "manual",
      size: i,
      version: "1",
      install_command: "",
      uninstall_command: "",
    })),
    managers: ["apt"],
    errors: [],
    caches: [],
    total: 205,
    total_size: 1000,
  } as any);
  const ui = await mount(providers(memory, <Packages />));
  await click(button(ui.container, "下一页"));
  await click(ui.container.querySelector("tbody tr"));
  await ui.render(providers(memory, <div />));
  await ui.render(providers(memory, <Packages />));
  expect(ui.container.querySelector("tbody tr")?.textContent).toContain(
    "package-100",
  );
  expect(ui.container.querySelector("tbody tr.selected")).not.toBeNull();
  const search = ui.container.querySelector<HTMLInputElement>(
    ".search-input input",
  )!;
  await input(search, "package-00");
  expect(ui.container.querySelector("tbody tr")?.textContent).toContain(
    "package-000",
  );
  expect(button(ui.container, "上一页").disabled).toBe(true);
  await ui.unmount();
  memory.dispose();
});
