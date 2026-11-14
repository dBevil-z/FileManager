import { it, expect } from "vitest";
import { repository } from "./About";
it("accepts an author profile or repository without inventing release links", () => {
  expect(repository("https://github.com/dBevil-z")?.isRepo).toBe(false);
  expect(repository("https://github.com/dBevil-z/FileManager/")?.isRepo).toBe(
    true,
  );
  for (const url of [
    "http://github.com/a/b",
    "https://evil.example/a/b",
    "https://user:secret@github.com/a/b",
    "https://github.com/a/b/issues",
    "https://github.com/a/b?q=private",
    "javascript:alert(1)",
  ])
    expect(repository(url)).toBeNull();
});
