import { it, expect } from "vitest";
import { skins, contrast, accentInk } from "./themes";
it("keeps normal and muted text readable on every skin surface in both modes", () => {
  for (const skin of skins) {
    for (const bg of skin.dark) {
      expect(contrast("#ededf0", bg), skin.id).toBeGreaterThanOrEqual(4.5);
      expect(contrast("#bcbcc7", bg), skin.id).toBeGreaterThanOrEqual(4.5);
    }
    for (const bg of skin.light) {
      expect(contrast("#26262d", bg), skin.id).toBeGreaterThanOrEqual(4.5);
      expect(contrast("#5a5a65", bg), skin.id).toBeGreaterThanOrEqual(4.5);
    }
    expect(
      contrast(skin.accent, accentInk(skin.accent)),
      skin.id,
    ).toBeGreaterThanOrEqual(4.5);
  }
});
