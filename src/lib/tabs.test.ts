import { describe, it, expect } from "vitest";
import { restoreTabs, tabsReducer } from "./tabs";
describe("folder tabs", () => {
  it("repairs invalid sessions and limits restored tabs", () => {
    expect(restoreTabs(null, null, "/home").items).toEqual([
      { id: "initial", path: "/home" },
    ]);
    const v = restoreTabs(
      Array.from({ length: 20 }, (_, i) => ({ id: String(i), path: "/d" + i })),
      "missing",
      "/",
    );
    expect(v.items).toHaveLength(12);
    expect(v.active).toBe("0");
    expect(
      restoreTabs(
        [
          { id: "a", path: "/a" },
          { id: "a", path: "/b" },
        ],
        "a",
        "/",
      ).items,
    ).toHaveLength(1);
  });
  it("keeps inactive paths intact and picks the adjacent tab on close", () => {
    let v = restoreTabs(
      [
        { id: "a", path: "/a" },
        { id: "b", path: "/b" },
      ],
      "a",
      "/",
    );
    v = tabsReducer(v, { type: "navigate", path: "/new" });
    expect(v.items[1].path).toBe("/b");
    v = tabsReducer(v, { type: "activate", id: "b" });
    v = tabsReducer(v, { type: "close", id: "b" });
    expect(v.active).toBe("a");
    expect(v.items[0].path).toBe("/new");
    expect(tabsReducer(v, { type: "close", id: "a" })).toBe(v);
  });
  it("bounds new tabs and ignores unknown activation", () => {
    const v = restoreTabs(
      Array.from({ length: 12 }, (_, i) => ({ id: String(i), path: "/" })),
      "0",
      "/",
    );
    expect(tabsReducer(v, { type: "open", id: "extra", path: "/extra" })).toBe(
      v,
    );
    expect(tabsReducer(v, { type: "activate", id: "unknown" })).toBe(v);
  });
});
