/**
 * useStorage — React hook for persistent key-value storage (issue #21).
 *
 * Reads the stored value for `key` on mount and returns a reactive tuple.
 * Writing via `setValue` is optimistic: local state updates immediately
 * while `storage.set` persists to disk in the background.
 *
 * @example
 * ```tsx
 * const [theme, setTheme, loading] = useStorage<string>("theme");
 * if (loading) return <Spinner />;
 * return <button onClick={() => setTheme("dark")}>{theme ?? "light"}</button>;
 * ```
 *
 * Requires `"storage": true` in `permissions.json`.
 */

import { useState, useEffect, useCallback } from "react";
import { get, set } from "../modules/storage";

/**
 * Reactive hook for a single persistent storage key.
 *
 * @param key - The storage key to read/write.
 * @returns A tuple of `[value, setValue, loading]`:
 *   - `value` — current stored value, or `null` if not set / not yet loaded.
 *   - `setValue` — stable setter; updates local state immediately and persists async.
 *   - `loading` — `true` during the initial async read, `false` once resolved.
 */
export function useStorage<T>(
  key: string
): [T | null, (value: T) => void, boolean] {
  const [value, setValueState] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);

  // Load stored value on mount (or when key changes).
  useEffect(() => {
    let cancelled = false;

    setLoading(true);
    setValueState(null);

    get(key)
      .then((stored) => {
        if (!cancelled) {
          // storage.get returns string | null; callers own serialization.
          setValueState(stored as unknown as T);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [key]);

  // Stable setter — identity unchanged across renders unless key changes.
  const setValue = useCallback(
    (newValue: T) => {
      // Optimistic update: reflect in UI immediately.
      setValueState(newValue);
      // Persist asynchronously; errors are swallowed here — surface via error
      // boundaries or wrap the hook for explicit error handling.
      set(key, newValue as unknown as string).catch(() => {
        // No-op: storage errors are best surfaced at the component level.
      });
    },
    [key]
  );

  return [value, setValue, loading];
}
