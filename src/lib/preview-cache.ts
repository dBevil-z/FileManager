// Cache resolved values by byte weight. In-flight requests are never retained
// after their last consumer has gone away, and obsolete queued work is skipped.
export class WeightedCache<T> {
  private values = new Map<string, { value: T; bytes: number }>();
  bytes = 0;
  constructor(
    readonly limit = 8 * 1024 * 1024,
    readonly count = 64,
  ) {}
  get(key: string) {
    const item = this.values.get(key);
    if (!item) return;
    this.values.delete(key);
    this.values.set(key, item);
    return item.value;
  }
  set(key: string, value: T, bytes: number) {
    this.delete(key);
    if (bytes > this.limit) return;
    this.values.set(key, { value, bytes });
    this.bytes += bytes;
    while (this.bytes > this.limit || this.values.size > this.count)
      this.delete(this.values.keys().next().value!);
  }
  delete(key: string) {
    const item = this.values.get(key);
    if (item) this.bytes -= item.bytes;
    this.values.delete(key);
  }
  clear() {
    this.values.clear();
    this.bytes = 0;
  }
  get size() {
    return this.values.size;
  }
}
