import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../services/api";
import type { AppState, SyncProgress } from "../types";

export function useAppState() {
  const [state, setState] = useState<AppState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [syncProgress, setSyncProgress] = useState<SyncProgress | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await api.getAppState();
      setState(next);
      setError(null);
      if (next.drive.sync?.active) {
        setSyncProgress(next.drive.sync);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<SyncProgress>("sync-progress", (event) => {
      if (cancelled) return;
      setSyncProgress(event.payload);
      if (!event.payload.active && event.payload.percent >= 100) {
        window.setTimeout(() => {
          if (!cancelled) setSyncProgress(null);
        }, 1200);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<AppState["drive"]>("drive-status", (event) => {
      if (cancelled) return;
      setState((prev) => (prev ? { ...prev, drive: event.payload } : prev));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen("app-state-changed", () => {
      if (!cancelled) void refresh();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refresh]);

  const run = useCallback(
    async <T,>(fn: () => Promise<T>) => {
      setBusy(true);
      setError(null);
      try {
        const result = await fn();
        await refresh();
        return result;
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        // Soft cancel / timeout from Google browser flow — don't leave a red banner.
        if (
          !/sign-in cancelled/i.test(msg) &&
          !/sign-in timed out/i.test(msg) &&
          !/access_denied/i.test(msg)
        ) {
          setError(msg);
        }
        setSyncProgress(null);
        throw e;
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  return { state, error, busy, syncProgress, refresh, run, setError };
}
