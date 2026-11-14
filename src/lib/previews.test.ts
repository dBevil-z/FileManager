import { it, expect, vi } from "vitest";
vi.mock("../api", () => ({ request: vi.fn() }));
import { request } from "../api";
import {
  loadPreview,
  clearPreviewCache,
  previewCacheStats,
  type PreviewData,
} from "./previews";
import type { Entry } from "../types";
const entry = (path: string) =>
  ({ path, inode: 1, device: 1, modified: 1, size: 1 }) as Entry;
it("skips a queued thumbnail whose consumer has unmounted", async () => {
  clearPreviewCache();
  let release!: (value: PreviewData) => void;
  vi.mocked(request).mockReturnValueOnce(
    new Promise<PreviewData>((resolve) => {
      release = resolve;
    }),
  );
  const first = loadPreview(entry("/first"), true);
  const abort = new AbortController();
  const second = loadPreview(entry("/obsolete"), true, abort.signal);
  const rejection = expect(second).rejects.toMatchObject({
    name: "AbortError",
  });
  abort.abort();
  await rejection;
  expect(previewCacheStats().queued).toBe(0);
  release({ kind: "text" });
  await first;
  await Promise.resolve();
  expect(vi.mocked(request)).toHaveBeenCalledTimes(1);
});
it("keeps thumbnail and full details separate and refuses oversized cached values", async () => {
  clearPreviewCache();
  vi.mocked(request).mockReset();
  vi.mocked(request)
    .mockResolvedValueOnce({ text: { text: "sample", revision: "" } })
    .mockResolvedValueOnce({
      text: { text: "x".repeat(5 * 1024 * 1024), revision: "full" },
    });
  const e = entry("/same");
  const small = await loadPreview(e, true);
  const full = await loadPreview(e);
  expect(small.text?.text).toBe("sample");
  expect(full.text?.revision).toBe("full");
  expect(previewCacheStats().bytes).toBeLessThan(8 * 1024 * 1024);
  expect(previewCacheStats().entries).toBe(1);
  expect(await loadPreview(e, true)).toBe(small);
});
