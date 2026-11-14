import { useLayoutEffect, type RefObject } from "react";
import { useViewMemory } from "./useViewState";
import { viewKey } from "../lib/view-memory";

const selector =
  ".feature-page,.tool-main,.tool-detail,.table-scroll,.file-scroll,.column-list,.columns-view,.remote-connections,.remote-table-wrap,.sidebar-scroll";
type Point = { top: number; left: number };

/** Retry after asynchronous content/virtual rows arrive; a user scroll wins. */
export function useViewScroll(
  root: RefObject<HTMLElement | null>,
  scope: string,
  ready = true,
  ancestor?: string,
  skipRestore = false,
) {
  const memory = useViewMemory();
  useLayoutEffect(() => {
    const container = ancestor
      ? root.current?.closest<HTMLElement>(ancestor)
      : root.current;
    if (!container || !ready) return;
    const pending = new Map<HTMLElement, Point>();
    const known = new Map<HTMLElement, string>();
    const applying = new WeakSet<HTMLElement>();
    const attempted = new WeakSet<HTMLElement>();
    let frame = 0;
    let userMoved = false;
    const restore = () => {
      for (const node of known.keys())
        if (node !== container && !container.contains(node)) {
          known.delete(node);
          pending.delete(node);
        }
      const nodes = [
        container,
        ...container.querySelectorAll<HTMLElement>(selector),
      ];
      const counts = new Map<string, number>();
      for (const node of nodes) {
        const name =
          node === container
            ? "root"
            : node.dataset.scrollKey ||
              [...node.classList].find((c) => c !== "selected") ||
              node.tagName;
        const index = counts.get(name) || 0;
        counts.set(name, index + 1);
        const key = viewKey("scroll", scope, name, index);
        if (!known.has(node)) {
          known.set(node, key);
          const saved = memory.get<Point>(key) || { top: 0, left: 0 };
          if (
            !skipRestore &&
            !userMoved &&
            saved &&
            Number.isFinite(saved.top) &&
            Number.isFinite(saved.left)
          )
            pending.set(node, saved);
        }
        const point = pending.get(node);
        if (point) {
          const arrived =
            Math.abs(node.scrollTop - point.top) < 2 &&
            Math.abs(node.scrollLeft - point.left) < 2;
          applying.add(node);
          node.scrollTop = point.top;
          node.scrollLeft = point.left;
          const reachable =
            Math.abs(node.scrollTop - point.top) < 2 &&
            Math.abs(node.scrollLeft - point.left) < 2;
          // Virtualizers initialize their own offset in a later layout effect.
          // Confirm the position survives a frame before finishing restoration.
          if (arrived && attempted.has(node)) pending.delete(node);
          else if (reachable) schedule();
          attempted.add(node);
          requestAnimationFrame(() => applying.delete(node));
        }
      }
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(restore);
    };
    const save = (node: HTMLElement) => {
      const key = known.get(node);
      if (key && !pending.has(node) && !applying.has(node))
        memory.set(key, { top: node.scrollTop, left: node.scrollLeft });
    };
    const scroll = (event: Event) => {
      if (event.target instanceof HTMLElement) save(event.target);
    };
    const override = () => {
      userMoved = true;
      pending.clear();
    };
    const saveAll = () => {
      for (const node of known.keys()) save(node);
    };
    window.addEventListener("filemanager-before-navigate", saveAll);
    container.addEventListener("scroll", scroll, true);
    container.addEventListener("wheel", override, { passive: true });
    container.addEventListener("touchstart", override, { passive: true });
    container.addEventListener("pointerdown", override);
    container.addEventListener("keydown", override);
    const mutation = new MutationObserver(schedule);
    mutation.observe(container, { childList: true, subtree: true });
    const resize = new ResizeObserver(schedule);
    resize.observe(container);
    restore();
    schedule();
    return () => {
      window.removeEventListener("filemanager-before-navigate", saveAll);
      cancelAnimationFrame(frame);
      mutation.disconnect();
      resize.disconnect();
      container.removeEventListener("scroll", scroll, true);
      container.removeEventListener("wheel", override);
      container.removeEventListener("touchstart", override);
      container.removeEventListener("pointerdown", override);
      container.removeEventListener("keydown", override);
    };
  }, [root, scope, ready, memory, ancestor, skipRestore]);
}
