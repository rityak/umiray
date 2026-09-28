import { type Dispatch, type SetStateAction, useCallback, useState } from "react";

const kept = new Map<string, unknown>();

export function useCached<T>(key: string, initial: T): [T, Dispatch<SetStateAction<T>>] {
  const [value, setValue] = useState<T>(() => (kept.has(key) ? (kept.get(key) as T) : initial));
  const set = useCallback<Dispatch<SetStateAction<T>>>(
    (next) =>
      setValue((current) => {
        const value = typeof next === "function" ? (next as (current: T) => T)(current) : next;
        kept.set(key, value);
        return value;
      }),
    [key],
  );
  return [value, set];
}
