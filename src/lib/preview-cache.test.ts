import { it, expect } from "vitest";
import { WeightedCache } from "./preview-cache";
it("evicts least-recently-used values by actual byte weight and count", () => {
  const cache = new WeightedCache<string>(10, 3);
  cache.set("a", "A", 4);
  cache.set("b", "B", 4);
  expect(cache.get("a")).toBe("A");
  cache.set("c", "C", 4);
  expect(cache.get("b")).toBeUndefined();
  expect(cache.bytes).toBe(8);
  cache.set("huge", "H", 20);
  expect(cache.size).toBe(2);
  cache.set("a", "new", 2);
  expect(cache.bytes).toBe(6);
  cache.clear();
  expect(cache.bytes).toBe(0);
});
