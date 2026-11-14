import { useCallback, useEffect, useRef, useState } from "react";
import { errorText } from "../api";

export function useAsync<T>(
  load: (signal: AbortSignal) => Promise<T>,
  deps: unknown[],
) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const generation = useRef(0);
  const abort = useRef<AbortController | null>(null);
  const loadRef = useRef(load);
  loadRef.current = load;
  const refresh = useCallback(async () => {
    abort.current?.abort();
    const controller = new AbortController();
    abort.current = controller;
    const current = ++generation.current;
    setLoading(true);
    setError("");
    try {
      const value = await loadRef.current(controller.signal);
      if (current === generation.current) setData(value);
    } catch (e) {
      if (current === generation.current) setError(errorText(e));
    } finally {
      if (current === generation.current) setLoading(false);
    }
  }, []);
  useEffect(() => {
    setData(null);
    void refresh();
    return () => {
      generation.current++;
      abort.current?.abort();
    };
  }, [...deps, refresh]);
  return { data, setData, error, loading, refresh };
}
