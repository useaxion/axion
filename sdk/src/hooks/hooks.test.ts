/**
 * SDK React hook tests — issue #23.
 *
 * Tests useStorage, useSystemInfo, and useWindow using
 * @testing-library/react renderHook + act. Capability modules are mocked
 * so no live Rust runtime is needed.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { useStorage } from "./useStorage";
import { useSystemInfo } from "./useSystemInfo";
import { useWindow } from "./useWindow";

// ── Mock the capability modules ───────────────────────────────────────────────

vi.mock("../modules/storage", () => ({
  get: vi.fn(),
  set: vi.fn(),
  remove: vi.fn(),
  storage: { get: vi.fn(), set: vi.fn(), remove: vi.fn() },
}));

vi.mock("../modules/system", () => ({
  info: vi.fn(),
  platform: vi.fn(),
  version: vi.fn(),
  system: { info: vi.fn(), platform: vi.fn(), version: vi.fn() },
}));

vi.mock("../modules/window", () => ({
  minimize: vi.fn(),
  maximize: vi.fn(),
  close: vi.fn(),
  setTitle: vi.fn(),
  window_: { minimize: vi.fn(), maximize: vi.fn(), close: vi.fn(), setTitle: vi.fn() },
}));

// Import mocked versions for assertions.
import * as storageMod from "../modules/storage";
import * as systemMod from "../modules/system";
import * as windowMod from "../modules/window";

// ── useStorage ────────────────────────────────────────────────────────────────

describe("useStorage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("returns null and loading=true on initial render", () => {
    vi.mocked(storageMod.get).mockReturnValue(new Promise(() => {})); // never resolves

    const { result } = renderHook(() => useStorage<string>("theme"));
    const [value, , loading] = result.current;

    expect(value).toBeNull();
    expect(loading).toBe(true);
  });

  it("resolves stored value and clears loading flag", async () => {
    vi.mocked(storageMod.get).mockResolvedValue("dark");

    const { result } = renderHook(() => useStorage<string>("theme"));

    await waitFor(() => {
      const [, , loading] = result.current;
      expect(loading).toBe(false);
    });

    const [value, , loading] = result.current;
    expect(value).toBe("dark");
    expect(loading).toBe(false);
  });

  it("resolves to null when key does not exist", async () => {
    vi.mocked(storageMod.get).mockResolvedValue(null);

    const { result } = renderHook(() => useStorage<string>("missing"));

    await waitFor(() => {
      const [, , loading] = result.current;
      expect(loading).toBe(false);
    });

    const [value] = result.current;
    expect(value).toBeNull();
  });

  it("setValue updates local state immediately (optimistic)", async () => {
    vi.mocked(storageMod.get).mockResolvedValue(null);
    vi.mocked(storageMod.set).mockResolvedValue(undefined);

    const { result } = renderHook(() => useStorage<string>("theme"));

    await waitFor(() => expect(result.current[2]).toBe(false));

    act(() => {
      result.current[1]("dark");
    });

    const [value] = result.current;
    expect(value).toBe("dark");
  });

  it("setValue calls storage.set with key and value", async () => {
    vi.mocked(storageMod.get).mockResolvedValue(null);
    vi.mocked(storageMod.set).mockResolvedValue(undefined);

    const { result } = renderHook(() => useStorage<string>("theme"));
    await waitFor(() => expect(result.current[2]).toBe(false));

    act(() => {
      result.current[1]("light");
    });

    expect(storageMod.set).toHaveBeenCalledWith("theme", "light");
  });

  it("setValue is stable across renders (same reference)", async () => {
    vi.mocked(storageMod.get).mockResolvedValue("dark");

    const { result, rerender } = renderHook(() => useStorage<string>("theme"));
    await waitFor(() => expect(result.current[2]).toBe(false));

    const setter1 = result.current[1];
    rerender();
    const setter2 = result.current[1];

    expect(setter1).toBe(setter2);
  });

  it("re-fetches when key changes", async () => {
    vi.mocked(storageMod.get)
      .mockResolvedValueOnce("dark")
      .mockResolvedValueOnce("light");

    const { result, rerender } = renderHook(
      ({ key }: { key: string }) => useStorage<string>(key),
      { initialProps: { key: "theme" } }
    );

    await waitFor(() => expect(result.current[2]).toBe(false));
    expect(result.current[0]).toBe("dark");

    rerender({ key: "accent" });
    await waitFor(() => expect(result.current[2]).toBe(false));
    expect(result.current[0]).toBe("light");

    expect(storageMod.get).toHaveBeenCalledTimes(2);
  });

  it("clears loading on storage.get error (does not crash)", async () => {
    vi.mocked(storageMod.get).mockRejectedValue(new Error("permission denied"));

    const { result } = renderHook(() => useStorage<string>("theme"));

    await waitFor(() => expect(result.current[2]).toBe(false));
    expect(result.current[0]).toBeNull();
  });
});

// ── useSystemInfo ─────────────────────────────────────────────────────────────

describe("useSystemInfo", () => {
  const fakeInfo = {
    os: "Windows",
    version: "10.0.22000",
    arch: "x86_64",
    hostname: "my-pc",
    totalMemoryMb: 16384,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("returns null on initial render", () => {
    vi.mocked(systemMod.info).mockReturnValue(new Promise(() => {}));

    const { result } = renderHook(() => useSystemInfo());
    expect(result.current).toBeNull();
  });

  it("resolves to SystemInfo after async call", async () => {
    vi.mocked(systemMod.info).mockResolvedValue(fakeInfo);

    const { result } = renderHook(() => useSystemInfo());

    await waitFor(() => expect(result.current).not.toBeNull());

    expect(result.current).toEqual(fakeInfo);
    expect(result.current?.os).toBe("Windows");
    expect(result.current?.totalMemoryMb).toBe(16384);
  });

  it("calls system.info exactly once on mount", async () => {
    vi.mocked(systemMod.info).mockResolvedValue(fakeInfo);

    const { rerender } = renderHook(() => useSystemInfo());
    await waitFor(() => expect(systemMod.info).toHaveBeenCalledTimes(1));

    rerender();
    rerender();

    expect(systemMod.info).toHaveBeenCalledTimes(1);
  });

  it("stays null if system.info rejects", async () => {
    vi.mocked(systemMod.info).mockRejectedValue(new Error("rpc error"));

    const { result } = renderHook(() => useSystemInfo());

    // Give the rejection time to propagate.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });

    expect(result.current).toBeNull();
  });
});

// ── useWindow ─────────────────────────────────────────────────────────────────

describe("useWindow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("returns an object with minimize, maximize, close, setTitle", () => {
    const { result } = renderHook(() => useWindow());
    expect(typeof result.current.minimize).toBe("function");
    expect(typeof result.current.maximize).toBe("function");
    expect(typeof result.current.close).toBe("function");
    expect(typeof result.current.setTitle).toBe("function");
  });

  it("returned object is stable across renders", () => {
    const { result, rerender } = renderHook(() => useWindow());
    const ref1 = result.current;
    rerender();
    const ref2 = result.current;
    expect(ref1).toBe(ref2);
  });

  it("minimize delegates to window module", async () => {
    vi.mocked(windowMod.minimize).mockResolvedValue(undefined);
    const { result } = renderHook(() => useWindow());
    await act(async () => { await result.current.minimize(); });
    expect(windowMod.minimize).toHaveBeenCalledOnce();
  });

  it("maximize delegates to window module", async () => {
    vi.mocked(windowMod.maximize).mockResolvedValue(undefined);
    const { result } = renderHook(() => useWindow());
    await act(async () => { await result.current.maximize(); });
    expect(windowMod.maximize).toHaveBeenCalledOnce();
  });

  it("close delegates to window module", async () => {
    vi.mocked(windowMod.close).mockResolvedValue(undefined);
    const { result } = renderHook(() => useWindow());
    await act(async () => { await result.current.close(); });
    expect(windowMod.close).toHaveBeenCalledOnce();
  });

  it("setTitle delegates to window module with correct arg", async () => {
    vi.mocked(windowMod.setTitle).mockResolvedValue(undefined);
    const { result } = renderHook(() => useWindow());
    await act(async () => { await result.current.setTitle("My App"); });
    expect(windowMod.setTitle).toHaveBeenCalledWith("My App");
  });
});
