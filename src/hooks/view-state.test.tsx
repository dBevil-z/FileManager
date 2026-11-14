// @vitest-environment jsdom
import { act, useRef } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ViewMemory, viewKey } from "../lib/view-memory";
import { useViewState } from "./useViewState";
import { useViewScroll } from "./useViewScroll";
import {
  button,
  click,
  installViewDOM,
  mount,
  providers,
  settle,
  cleanupUI,
} from "../test/view-ui";

beforeEach(() => {
  installViewDOM();
});
afterEach(async () => {
  await cleanupUI();
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

it("keeps controls through unmount and independent directory/tab changes", async () => {
  const memory = new ViewMemory();
  function Control({ scope }: { scope: string }) {
    const [count, setCount] = useViewState(scope, 0);
    return (
      <button
        onClick={() => {
          setCount((v) => v + 1);
          setCount((v) => v + 1);
        }}
      >
        {count}
      </button>
    );
  }
  const ui = await mount(providers(memory, <Control scope="folder:a" />));
  await click(button(ui.container, "0"));
  expect(button(ui.container, "2")).toBeDefined();
  await ui.render(providers(memory, <Control scope="folder:b" />));
  await click(button(ui.container, "0"));
  await ui.render(providers(memory, <div>Other menu</div>));
  await ui.render(providers(memory, <Control scope="folder:a" />));
  expect(button(ui.container, "2")).toBeDefined();
  await ui.unmount();
  const restored = await mount(
    providers(new ViewMemory(memory.snapshot()), <Control scope="folder:b" />),
  );
  expect(button(restored.container, "2")).toBeDefined();
  await restored.unmount();
});

it("restores scroll after asynchronous content arrives and starts new scopes at the top", async () => {
  const resize = installViewDOM();
  const memory = new ViewMemory();
  const scrollKey = viewKey("scroll", "one", "root", 0);
  memory.set(scrollKey, { top: 640, left: 0 });
  let height = 0,
    top = 0;
  function Scroller({ scope, content }: { scope: string; content: boolean }) {
    const ref = useRef<HTMLDivElement>(null);
    useViewScroll(ref, scope);
    return (
      <div
        ref={(node) => {
          ref.current = node;
          if (node)
            Object.defineProperty(node, "scrollTop", {
              configurable: true,
              get: () => top,
              set: (v) => {
                top = Math.min(height, Math.max(0, v));
              },
            });
        }}
      >
        {content && <div>Loaded</div>}
      </div>
    );
  }
  const ui = await mount(
    providers(memory, <Scroller scope="one" content={false} />),
  );
  expect(top).toBe(0);
  expect(memory.get(scrollKey)).toEqual({ top: 640, left: 0 });
  height = 1000;
  await ui.render(providers(memory, <Scroller scope="one" content />));
  await settle();
  expect(top).toBe(640);
  await ui.render(providers(memory, <Scroller scope="two" content />));
  expect(top).toBe(0);
  await act(async () => {
    top = 240;
    ui.container.firstElementChild!.dispatchEvent(new Event("scroll"));
  });
  await ui.render(providers(memory, <Scroller scope="one" content />));
  expect(top).toBe(640);
  await ui.render(providers(memory, <Scroller scope="two" content />));
  expect(top).toBe(240);
  resize();
  await settle();
  await ui.unmount();
});

it("lets manual scrolling override pending restoration without saving a clamped zero on unmount", async () => {
  const memory = new ViewMemory();
  const key = viewKey("scroll", "pending", "root", 0);
  memory.set(key, { top: 500, left: 0 });
  let height = 0,
    top = 0;
  function Scroller() {
    const ref = useRef<HTMLDivElement>(null);
    useViewScroll(ref, "pending");
    return (
      <div
        ref={(node) => {
          ref.current = node;
          if (node)
            Object.defineProperty(node, "scrollTop", {
              configurable: true,
              get: () => top,
              set: (v) => {
                top = Math.min(height, v);
              },
            });
        }}
      />
    );
  }
  const ui = await mount(providers(memory, <Scroller />));
  await ui.render(providers(memory, <div />));
  expect(memory.get(key)).toEqual({ top: 500, left: 0 });
  await ui.render(providers(memory, <Scroller />));
  const node = ui.container.firstElementChild!;
  await act(async () => {
    node.dispatchEvent(new Event("wheel"));
    height = 1000;
    top = 120;
    node.dispatchEvent(new Event("scroll"));
  });
  node.append(document.createElement("div"));
  await settle();
  expect(top).toBe(120);
  expect(memory.get(key)).toEqual({ top: 120, left: 0 });
  await ui.unmount();
  memory.dispose();
});
