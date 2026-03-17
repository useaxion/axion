/**
 * useSystemInfo — React hook for OS and hardware information (issue #22).
 *
 * Fetches system info once on mount. Returns `null` during the initial
 * async load, then the typed `SystemInfo` object once resolved.
 *
 * No permissions required.
 *
 * @example
 * ```tsx
 * const info = useSystemInfo();
 * if (!info) return <Spinner />;
 * return <p>{info.os} {info.version} — {info.totalMemoryMb} MB RAM</p>;
 * ```
 */

import { useState, useEffect } from "react";
import { info } from "../modules/system";
import type { SystemInfo } from "../modules/system";

/**
 * Fetches system information once on mount.
 *
 * @returns `SystemInfo` after the async call resolves, `null` while loading.
 */
export function useSystemInfo(): SystemInfo | null {
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);

  useEffect(() => {
    let cancelled = false;

    info()
      .then((data) => {
        if (!cancelled) {
          setSystemInfo(data);
        }
      })
      .catch(() => {
        // Errors are swallowed — callers that need error handling should
        // call system.info() directly via the capability module.
      });

    return () => {
      cancelled = true;
    };
  }, []); // Empty deps: fetch once on mount only.

  return systemInfo;
}
