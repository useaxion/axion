/**
 * useWindow — React hook for native window control (issue #22).
 *
 * Returns a **stable** object of window control functions. The object
 * reference never changes across renders, so it is safe to pass as a
 * prop or include in dependency arrays without causing re-renders.
 *
 * No permissions required.
 *
 * @example
 * ```tsx
 * const win = useWindow();
 * return (
 *   <header>
 *     <button onClick={win.minimize}>_</button>
 *     <button onClick={win.maximize}>□</button>
 *     <button onClick={win.close}>×</button>
 *   </header>
 * );
 * ```
 */

import { useMemo } from "react";
import { minimize, maximize, close, setTitle } from "../modules/window";

/** Stable window control API returned by `useWindow`. */
export interface WindowControls {
  /** Minimize the window to the taskbar. */
  minimize: () => Promise<void>;
  /** Maximize the window, or restore it if already maximized. */
  maximize: () => Promise<void>;
  /** Close the window and exit the application. */
  close: () => Promise<void>;
  /** Set the window title bar text. */
  setTitle: (title: string) => Promise<void>;
}

/**
 * Returns stable window control functions.
 *
 * The returned object is memoized with an empty dependency array —
 * its identity is constant for the lifetime of the component.
 */
export function useWindow(): WindowControls {
  return useMemo(
    () => ({ minimize, maximize, close, setTitle }),
    [] // module-level functions are stable references — memo never invalidates.
  );
}
