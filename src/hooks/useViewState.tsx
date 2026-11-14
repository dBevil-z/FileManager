import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { ViewMemory } from "../lib/view-memory";

export const ViewMemoryContext = createContext<ViewMemory | null>(null);
export function useViewMemory() {
  const memory = useContext(ViewMemoryContext);
  if (!memory) throw new Error("View memory is unavailable");
  return memory;
}

/** Persist small view controls; loaded file data and form secrets stay local. */
export function useViewState<T>(
  key: string,
  initial: T | (() => T),
  validate?: (value: unknown) => value is T,
): [T, Dispatch<SetStateAction<T>>] {
  const memory = useViewMemory();
  const read = () => {
    const fallback =
      typeof initial === "function" ? (initial as () => T)() : initial;
    const saved = memory.get<T>(key);
    return saved !== undefined &&
      (validate
        ? validate(saved)
        : fallback === null
          ? saved === null || typeof saved === "string"
          : Array.isArray(fallback)
            ? Array.isArray(saved)
            : typeof saved === typeof fallback)
      ? saved
      : fallback;
  };
  const [state, setState] = useState(() => ({ key, value: read() }));
  const value = state.key === key ? state.value : read();
  if (state.key !== key) setState({ key, value });
  const latest = useRef({ key, value });
  latest.current = { key, value };
  const setValue = useCallback<Dispatch<SetStateAction<T>>>(
    (next) => {
      const previous = latest.current.value;
      const value =
        typeof next === "function" ? (next as (v: T) => T)(previous) : next;
      latest.current = { key, value };
      memory.set(key, value);
      setState({ key, value });
    },
    [key, memory],
  );
  useLayoutEffect(() => {
    memory.set(key, value);
  }, [memory, key, value]);
  return [value, setValue];
}
