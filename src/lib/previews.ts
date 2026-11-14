import { request } from "../api";
import type { Entry } from "../types";
import { WeightedCache } from "./preview-cache";
export type PreviewData = {
  entry?: Entry;
  kind?: string;
  thumbnail?: string;
  media?: string;
  mime?: string;
  metadata?: Record<string, unknown>;
  warning?: string;
  text?: { text: string; revision: string };
  children?: Entry[];
  tree?: { entry: Entry; children?: unknown[] }[];
};
const cache = new WeightedCache<PreviewData>();
type Work = {
  key: string;
  start: () => void;
  waiters: number;
  started: boolean;
};
const pending = new Map<
  string,
  { promise: Promise<PreviewData>; work: Work }
>();
const queue: Work[] = [];
let active = 0;
let epoch = 0;
function pump() {
  while (active < 1 && queue.length) {
    const work = queue.shift()!;
    work.started = true;
    work.start();
  }
}
export function previewCacheStats() {
  return {
    entries: cache.size,
    bytes: cache.bytes,
    queued: queue.filter((w) => w.waiters > 0).length,
    active,
  };
}
export function clearPreviewCache() {
  epoch++;
  cache.clear();
}
export function loadPreview(
  entry: Entry,
  thumbnail = false,
  signal?: AbortSignal,
): Promise<PreviewData> {
  const key = `${thumbnail}:${entry.path}:${entry.inode}:${entry.device}:${entry.modified}:${entry.size}`;
  if (signal?.aborted)
    return Promise.reject(new DOMException("Aborted", "AbortError"));
  const cached = cache.get(key);
  if (cached) return Promise.resolve(cached);
  let task = pending.get(key);
  if (!task) {
    const generation = epoch;
    let run!: () => void;
    const work: Work = { key, waiters: 0, started: false, start: () => run() };
    const promise = new Promise<PreviewData>((resolve, reject) => {
      run = () => {
        if (!work.waiters) {
          pending.delete(key);
          reject(new DOMException("Aborted", "AbortError"));
          return;
        }
        active++;
        request<PreviewData>("preview", { path: entry.path, thumbnail })
          .then((value) => {
            if (generation === epoch && work.waiters > 0)
              cache.set(key, value, JSON.stringify(value).length * 2);
            resolve(value);
          }, reject)
          .finally(() => {
            active--;
            if (pending.get(key)?.work === work) pending.delete(key);
            pump();
          });
      };
    });
    task = { promise, work };
    pending.set(key, task);
    if (thumbnail) queue.push(work);
    else queue.unshift(work);
  }
  const { work, promise } = task;
  work.waiters++;
  const result = new Promise<PreviewData>((resolve, reject) => {
    let done = false;
    const finish = () => {
      if (done) return false;
      done = true;
      work.waiters--;
      signal?.removeEventListener("abort", abort);
      return true;
    };
    const abort = () => {
      if (finish()) {
        reject(new DOMException("Aborted", "AbortError"));
        if (!work.waiters && work.started) {
          if (pending.get(key)?.work === work) pending.delete(key);
        }
        if (!work.waiters && !work.started) {
          const index = queue.indexOf(work);
          if (index >= 0) queue.splice(index, 1);
          work.started = true;
          work.start();
        }
      }
    };
    signal?.addEventListener("abort", abort, { once: true });
    promise.then(
      (v) => {
        if (finish()) resolve(v);
      },
      (e) => {
        if (finish()) reject(e);
      },
    );
  });
  pump();
  return result;
}
