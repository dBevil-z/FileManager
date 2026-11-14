import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, vi } from "vitest";
import { AppContext, type AppContextValue } from "../context";
import { ViewMemoryContext } from "../hooks/useViewState";
import { ViewMemory } from "../lib/view-memory";
import { defaultSettings, emptyCatalog, type Bootstrap } from "../types";

export const boot: Bootstrap = {
  home: "/home/demo",
  data_dir: "/tmp/filemanager-test",
  version: "test",
  platform: "linux",
  places: [
    { id: "downloads", name: "下载", path: "/home/demo/Downloads" },
    { id: "documents", name: "文档", path: "/home/demo/Documents" },
    { id: "home", name: "主目录", path: "/home/demo" },
    { id: "root", name: "文件系统", path: "/" },
  ],
  catalog: { revision: 0, data: emptyCatalog },
  settings: { revision: 0, data: defaultSettings },
  session: { revision: 0, data: {} },
};
export function appContext(
  overrides: Partial<AppContextValue> = {},
): AppContextValue {
  return {
    boot,
    catalog: emptyCatalog,
    settings: defaultSettings,
    page: { type: "remote" },
    activeMenu: { type: "remote" },
    jobs: [],
    plugins: [],
    refresh: 0,
    navigate: vi.fn(),
    switchMenu: vi.fn(),
    resetViewMemory: vi.fn(),
    restartForUpdate: vi.fn(async () => {}),
    openTab: vi.fn(),
    runPlugin: vi.fn(),
    updateCatalog: vi.fn(async () => {}),
    updateSettings: vi.fn(async () => {}),
    notify: vi.fn(),
    prompt: vi.fn(async () => null),
    confirm: vi.fn(async () => false),
    pickFiles: vi.fn(async () => []),
    showModal: vi.fn(),
    refreshAll: vi.fn(),
    open: vi.fn(),
    peek: vi.fn(),
    operate: vi.fn(async () => null),
    saveText: vi.fn(async () => {}),
    ...overrides,
  };
}
export function providers(
  memory: ViewMemory,
  children: ReactNode,
  app = appContext(),
) {
  return (
    <ViewMemoryContext.Provider value={memory}>
      <AppContext.Provider value={app}>{children}</AppContext.Provider>
    </ViewMemoryContext.Provider>
  );
}
export function installViewDOM() {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const resize = new Set<ResizeObserverCallback>();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      callback: ResizeObserverCallback;
      constructor(callback: ResizeObserverCallback) {
        this.callback = callback;
        resize.add(callback);
      }
      observe() {}
      unobserve() {}
      disconnect() {
        resize.delete(this.callback);
      }
    },
  );
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  vi.stubGlobal("requestAnimationFrame", (fn: FrameRequestCallback) =>
    setTimeout(() => fn(performance.now()), 0),
  );
  vi.stubGlobal("cancelAnimationFrame", (id: number) => clearTimeout(id));
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }));
  Object.defineProperty(HTMLElement.prototype, "scrollTo", {
    configurable: true,
    value: function (this: HTMLElement, options: ScrollToOptions) {
      if (options.top !== undefined) this.scrollTop = options.top;
      if (options.left !== undefined) this.scrollLeft = options.left;
    },
  });
  return () => {
    for (const callback of resize) callback([], {} as ResizeObserver);
  };
}
export async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 5));
  });
}
export async function click(node: Element | null, type = "click") {
  if (!node) throw new Error("Missing interaction target");
  await act(async () => {
    node.dispatchEvent(new MouseEvent(type, { bubbles: true }));
  });
  await settle();
}
export async function input(
  node: HTMLInputElement | HTMLTextAreaElement,
  value: string,
) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      node instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype,
      "value",
    )!.set!.call(node, value);
    node.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await settle();
}
export function button(container: ParentNode, text: string) {
  const found = [
    ...container.querySelectorAll<HTMLButtonElement>("button"),
  ].find(
    (node) =>
      node.textContent?.trim() === text ||
      node.getAttribute("aria-label") === text ||
      node.querySelector("span")?.textContent?.trim() === text,
  );
  if (!found) throw new Error(`Missing button: ${text}`);
  return found;
}
const roots = new Map<Root, HTMLElement>();
export async function cleanupUI() {
  for (const [root, container] of roots) {
    await act(async () => root.unmount());
    container.remove();
  }
  roots.clear();
}
afterEach(cleanupUI);
export async function mount(children: ReactNode) {
  const container = document.createElement("div");
  document.body.append(container);
  const root: Root = createRoot(container);
  roots.set(root, container);
  const render = async (next: ReactNode) => {
    await act(async () => root.render(next));
    await settle();
  };
  await render(children);
  return {
    container,
    render,
    unmount: async () => {
      await act(async () => root.unmount());
      roots.delete(root);
      container.remove();
    },
  };
}
