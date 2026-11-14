export type FolderTab = { id: string; path: string };
export type FolderTabs = { items: FolderTab[]; active: string };
export type TabAction =
  | { type: "navigate"; path: string }
  | { type: "open"; path: string; id: string }
  | { type: "activate"; id: string }
  | { type: "close"; id: string };
export function restoreTabs(
  items: unknown,
  active: unknown,
  fallback: string,
): FolderTabs {
  const seen = new Set<string>();
  const tabs = Array.isArray(items)
    ? items
        .filter(
          (v): v is FolderTab =>
            v &&
            typeof v.id === "string" &&
            typeof v.path === "string" &&
            v.path.length > 0 &&
            !seen.has(v.id) &&
            Boolean(seen.add(v.id)),
        )
        .slice(0, 12)
    : [];
  if (!tabs.length) tabs.push({ id: "initial", path: fallback });
  return {
    items: tabs,
    active: tabs.some((t) => t.id === active) ? String(active) : tabs[0].id,
  };
}
export function tabsReducer(state: FolderTabs, action: TabAction): FolderTabs {
  if (action.type === "navigate")
    return {
      ...state,
      items: state.items.map((t) =>
        t.id === state.active ? { ...t, path: action.path } : t,
      ),
    };
  if (action.type === "open") {
    if (state.items.length >= 12) return state;
    return {
      items: [...state.items, { id: action.id, path: action.path }],
      active: action.id,
    };
  }
  if (action.type === "activate")
    return state.items.some((t) => t.id === action.id)
      ? { ...state, active: action.id }
      : state;
  if (state.items.length === 1) return state;
  const index = state.items.findIndex((t) => t.id === action.id);
  if (index < 0) return state;
  const items = state.items.filter((t) => t.id !== action.id);
  return {
    items,
    active:
      state.active === action.id
        ? items[Math.min(index, items.length - 1)].id
        : state.active,
  };
}
