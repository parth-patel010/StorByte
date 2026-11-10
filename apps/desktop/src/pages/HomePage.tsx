import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useSearchParams } from "react-router-dom";
import * as api from "../services/api";
import type { AccountInfo, DriveStatus, FilePreview, NodeInfo, SyncProgress } from "../types";

type Props = {
  drive: DriveStatus;
  accounts: AccountInfo[];
  busy: boolean;
  syncProgress: SyncProgress | null;
  onAddAccount: () => void | Promise<unknown>;
  onMount: () => void;
  onUnmount: () => void;
  onDisconnect: (id: string) => void;
  onReconnect: (id: string) => void;
  onRemove: (id: string) => void;
  onRefreshQuotas?: () => Promise<unknown>;
};

type ViewMode = "list" | "grid";

const EMPTY_FOLDER_SRC = "/intro/Homepage/empty_folder.png";

function formatBytes(n: number | null | undefined) {
  if (n == null || n < 0) return "—";
  if (n === 0) return "0 KB";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

function formatModified(iso: string | null | undefined) {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  const pad = (n: number) => String(n).padStart(2, "0");
  // Local timezone from Drive's UTC modifiedTime
  return `${pad(d.getDate())}-${pad(d.getMonth() + 1)}-${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fileTypeLabel(node: NodeInfo) {
  if (node.is_directory) return "File folder";
  const mime = node.mime_type ?? "";
  if (mime.includes("image")) return "Image";
  if (mime.includes("video")) return "Video";
  if (mime.includes("audio")) return "Audio";
  if (mime.includes("pdf")) return "PDF";
  if (mime.includes("zip") || mime.includes("compressed")) return "Compressed folder";
  if (mime.includes("spreadsheet") || mime.includes("excel")) return "Spreadsheet";
  if (mime.includes("document") || mime.includes("word")) return "Document";
  const ext = node.name.includes(".") ? node.name.split(".").pop()?.toUpperCase() : null;
  return ext ? `${ext} File` : "File";
}

function isImageNode(node: NodeInfo) {
  if (node.is_directory) return false;
  const mime = (node.mime_type ?? "").toLowerCase();
  if (mime.startsWith("image/")) return true;
  const n = node.name.toLowerCase();
  return [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".heic", ".avif"].some((ext) =>
    n.endsWith(ext),
  );
}

function FileIcon({ folder }: { folder: boolean }) {
  if (folder) {
    return (
      <svg width="16" height="16" viewBox="0 0 16 16" className="shrink-0 text-[#e8a317]" fill="currentColor">
        <path d="M1.5 3.5A1.5 1.5 0 0 1 3 2h3.17c.4 0 .78.16 1.06.44L8.5 3.7H13A1.5 1.5 0 0 1 14.5 5.2v7.3A1.5 1.5 0 0 1 13 14H3A1.5 1.5 0 0 1 1.5 12.5v-9Z" />
      </svg>
    );
  }
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" className="shrink-0 text-[#9a9aa0]" fill="currentColor">
      <path d="M3.5 1.5A1.5 1.5 0 0 0 2 3v10a1.5 1.5 0 0 0 1.5 1.5h9A1.5 1.5 0 0 0 14 13V5.5L9.5 1.5h-6Z" />
      <path d="M9.5 1.5V5H14" fill="none" stroke="currentColor" strokeWidth="1.2" />
    </svg>
  );
}

function StorageIcon({ className }: { className?: string }) {
  return (
    <svg className={className} width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
      <ellipse cx="8" cy="3.5" rx="5.2" ry="1.9" stroke="currentColor" strokeWidth="1.4" />
      <path
        d="M2.8 3.5v2.8c0 1.05 2.33 1.9 5.2 1.9s5.2-.85 5.2-1.9V3.5"
        stroke="currentColor"
        strokeWidth="1.4"
      />
      <path
        d="M2.8 6.3v2.8c0 1.05 2.33 1.9 5.2 1.9s5.2-.85 5.2-1.9V6.3"
        stroke="currentColor"
        strokeWidth="1.4"
      />
      <path
        d="M2.8 9.1v2.8c0 1.05 2.33 1.9 5.2 1.9s5.2-.85 5.2-1.9V9.1"
        stroke="currentColor"
        strokeWidth="1.4"
      />
    </svg>
  );
}

function InfoIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden
    >
      <circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.35" />
      <path
        d="M8 7.15v3.6"
        stroke="currentColor"
        strokeWidth="1.35"
        strokeLinecap="round"
      />
      <circle cx="8" cy="5.15" r="0.85" fill="currentColor" />
    </svg>
  );
}

function PlusIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden
    >
      <path
        d="M8 3.25v9.5M3.25 8h9.5"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

function AccountSourceInfo({
  email,
  tipId,
}: {
  email: string;
  tipId: string;
  placement?: "left" | "top" | "bottom" | "right";
}) {
  const triggerRef = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState({ top: 0, left: 0, placeAbove: true });

  const updatePos = useCallback(() => {
    const el = triggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const placeAbove = r.top > 48;
    setPos({
      top: placeAbove ? r.top - 8 : r.bottom + 8,
      left: Math.min(window.innerWidth - 16, Math.max(16, r.left + r.width / 2)),
      placeAbove,
    });
  }, []);

  const show = useCallback(() => {
    updatePos();
    setOpen(true);
  }, [updatePos]);

  useEffect(() => {
    if (!open) return;
    const onScroll = () => updatePos();
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
    };
  }, [open, updatePos]);

  return (
    <span
      ref={triggerRef}
      className="file-src-info relative inline-flex shrink-0 items-center justify-center"
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
      onMouseDown={(e) => e.stopPropagation()}
      onMouseEnter={show}
      onMouseLeave={() => setOpen(false)}
      onFocus={show}
      onBlur={() => setOpen(false)}
    >
      <span
        className="inline-flex h-4 w-4 items-center justify-center rounded-full text-[#9a9aa0] outline-none transition-colors hover:text-[#555]"
        aria-label={`From ${email}`}
        aria-describedby={open ? tipId : undefined}
        tabIndex={0}
      >
        <InfoIcon className="h-3.5 w-3.5" />
      </span>
      {open &&
        createPortal(
          <span
            id={tipId}
            role="tooltip"
            className="pointer-events-none fixed z-[200] w-max max-w-[240px] truncate rounded-md bg-[#2a2a2a] px-2.5 py-1.5 text-[12px] font-medium leading-none text-white shadow-md"
            style={{
              top: pos.top,
              left: pos.left,
              transform: pos.placeAbove ? "translate(-50%, -100%)" : "translate(-50%, 0)",
            }}
          >
            {email}
            <span
              className={`absolute left-1/2 h-0 w-0 -translate-x-1/2 border-x-[5px] border-x-transparent ${
                pos.placeAbove
                  ? "top-full border-t-[6px] border-t-[#2a2a2a]"
                  : "bottom-full border-b-[6px] border-b-[#2a2a2a]"
              }`}
              aria-hidden
            />
          </span>,
          document.body,
        )}
    </span>
  );
}

export function HomePage({
  drive,
  accounts,
  busy,
  syncProgress,
  onAddAccount,
  onRemove,
  onRefreshQuotas,
  onReconnect,
}: Props) {
  const [params] = useSearchParams();
  const locId = params.get("loc") || "all";
  const title = locId === "google" ? "Google Drive" : "All";

  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [path, setPath] = useState("\\");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [view, setView] = useState<ViewMode>("list");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadingDir, setLoadingDir] = useState(false);
  const [accountsOpen, setAccountsOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [searchResults, setSearchResults] = useState<NodeInfo[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [fetchingFolder, setFetchingFolder] = useState(false);
  const [showFetching, setShowFetching] = useState(false);
  const [preview, setPreview] = useState<FilePreview | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [signingIn, setSigningIn] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [previewBusy, setPreviewBusy] = useState(false);
  const cacheRef = useRef<Map<string, NodeInfo[]>>(new Map());
  const pathRef = useRef(path);
  const searchTimer = useRef<number | null>(null);
  const fetchHintTimer = useRef<number | null>(null);
  pathRef.current = path;

  const connectedAccounts = useMemo(
    () => accounts.filter((a) => a.status === "connected"),
    [accounts],
  );

  useEffect(() => {
    if (connectedAccounts.length === 0) {
      cacheRef.current.clear();
      setNodes([]);
      setPath("\\");
      setSelectedId(null);
      setPreview(null);
      setLoadError(null);
      setAccountsOpen(false);
    }
  }, [connectedAccounts.length]);

  const syncing = Boolean(syncProgress?.active || drive.indexing);
  const accountEmailById = useMemo(
    () => new Map(accounts.map((a) => [a.id, a.email] as const)),
    [accounts],
  );
  // Merge usage/quota across connected accounts only (ignore signed-out leftovers).
  const storageUsed = connectedAccounts.reduce(
    (sum, a) => sum + Math.max(0, a.usage_bytes ?? 0),
    0,
  );
  const storageTotal = connectedAccounts.reduce(
    (sum, a) => sum + Math.max(0, a.quota_bytes ?? 0),
    0,
  );
  const storagePercent =
    storageTotal > 0
      ? Math.min(100, Math.max(0, (storageUsed / storageTotal) * 100))
      : 0;
  const storagePercentLabel =
    storagePercent > 0 && storagePercent < 1
      ? "<1%"
      : `${Math.round(storagePercent)}%`;
  const storageDeg = (storagePercent / 100) * 360;
  const storageUsage = `${formatBytes(storageUsed)} / ${formatBytes(storageTotal)}`;

  const loadDirectory = useCallback(async (parent: string, opts?: { force?: boolean }) => {
    const cached = cacheRef.current.get(parent);
    if (cached && !opts?.force) {
      setNodes(cached);
      setLoadError(null);
      setLoadingDir(false);
      // Refresh from Drive for this folder only, then re-list.
      setFetchingFolder(true);
      void api
        .ensureDirectory(parent)
        .then(() => api.listDirectory(parent))
        .then((rows) => {
          cacheRef.current.set(parent, rows);
          if (pathRef.current === parent) setNodes(rows);
        })
        .catch(() => undefined)
        .finally(() => {
          if (pathRef.current === parent) setFetchingFolder(false);
        });
      return;
    }

    setLoadingDir(true);
    setFetchingFolder(true);
    try {
      await api.ensureDirectory(parent);
      const rows = await api.listDirectory(parent);
      cacheRef.current.set(parent, rows);
      if (pathRef.current === parent) {
        setNodes(rows);
        setLoadError(null);
      }
    } catch (e) {
      if (pathRef.current === parent) {
        setLoadError(e instanceof Error ? e.message : String(e));
        setNodes([]);
      }
    } finally {
      if (pathRef.current === parent) {
        setLoadingDir(false);
        setFetchingFolder(false);
      }
    }
  }, []);

  useEffect(() => {
    if (fetchHintTimer.current !== null) {
      window.clearTimeout(fetchHintTimer.current);
      fetchHintTimer.current = null;
    }
    if (!fetchingFolder) {
      setShowFetching(false);
      return;
    }
    // Only show spinner if fetch takes a moment — avoids header flicker.
    fetchHintTimer.current = window.setTimeout(() => setShowFetching(true), 280);
    return () => {
      if (fetchHintTimer.current !== null) {
        window.clearTimeout(fetchHintTimer.current);
        fetchHintTimer.current = null;
      }
    };
  }, [fetchingFolder]);

  useEffect(() => {
    void loadDirectory(path);
  }, [path, loadDirectory]);

  useEffect(() => {
    if (!syncing) {
      cacheRef.current.clear();
      void loadDirectory(path, { force: true });
    }
  }, [syncing]);

  useEffect(() => {
    void onRefreshQuotas?.();
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisten: { current?: () => void } = {};
    void listen("drive-changed", () => {
      if (cancelled) return;
      cacheRef.current.clear();
      void loadDirectory(pathRef.current, { force: true });
    }).then((fn) => {
      unlisten.current = fn;
    });
    return () => {
      cancelled = true;
      unlisten.current?.();
    };
  }, [loadDirectory]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (disposed) return;
        const kind = event.payload.type;
        if (kind === "enter" || kind === "over") {
          setDragOver(true);
        } else if (kind === "leave") {
          setDragOver(false);
        } else if (kind === "drop") {
          setDragOver(false);
          const paths = event.payload.paths ?? [];
          if (paths.length === 0) return;
          if (connectedAccounts.length === 0) {
            setLoadError("Connect a Google account before dropping files.");
            return;
          }
          setUploading(true);
          setLoadError(null);
          void api
            .uploadLocalPaths(paths, pathRef.current)
            .then(async () => {
              cacheRef.current.delete(pathRef.current);
              await loadDirectory(pathRef.current, { force: true });
            })
            .catch((e) => {
              const msg = e instanceof Error ? e.message : String(e);
              if (!/upload cancelled/i.test(msg)) {
                setLoadError(msg);
              }
            })
            .finally(() => setUploading(false));
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [accounts.length, loadDirectory]);

  useEffect(() => {
    if (searchTimer.current !== null) window.clearTimeout(searchTimer.current);
    const q = search.trim();
    if (!q) {
      setSearchResults(null);
      setSearching(false);
      return;
    }
    setSearching(true);
    searchTimer.current = window.setTimeout(() => {
      void api
        .searchNodes(q)
        .then((rows) => setSearchResults(rows))
        .catch(() => setSearchResults([]))
        .finally(() => setSearching(false));
    }, 180);
    return () => {
      if (searchTimer.current !== null) window.clearTimeout(searchTimer.current);
    };
  }, [search]);

  useEffect(() => {
    return () => {
      if (previewUrl) URL.revokeObjectURL(previewUrl);
    };
  }, [previewUrl]);

  const crumbs = path === "\\" ? [] : path.split("\\").filter(Boolean);
  const displayNodes = searchResults ?? nodes;
  const isSearching = search.trim().length > 0;
  const isEmpty = displayNodes.length === 0 && !syncing && !loadingDir && !searching && !fetchingFolder;

  const closePreview = () => {
    setPreview(null);
    setPreviewBusy(false);
    setPreviewUrl((prev) => {
      if (prev) URL.revokeObjectURL(prev);
      return null;
    });
  };

  const handleImport = async () => {
    if (signingIn) {
      await api.cancelGoogleOauth();
      return;
    }
    setSigningIn(true);
    try {
      await onAddAccount();
    } catch {
      // Parent surfaces error via app state.
    } finally {
      setSigningIn(false);
    }
  };

  const openImagePreview = async (node: NodeInfo) => {
    setSelectedId(node.id);
    setPreviewUrl((prev) => {
      if (prev) URL.revokeObjectURL(prev);
      return null;
    });
    setPreview(null);
    setPreviewBusy(true);
    try {
      const data = await api.readFilePreview(node.virtual_path);
      const bin = Uint8Array.from(atob(data.data_base64), (c) => c.charCodeAt(0));
      const blob = new Blob([bin], { type: data.mime_type || "image/*" });
      const url = URL.createObjectURL(blob);
      setPreview(data);
      setPreviewUrl(url);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    } finally {
      setPreviewBusy(false);
    }
  };

  const openNode = (node: NodeInfo) => {
    if (node.is_directory) {
      const next = node.virtual_path;
      setSearch("");
      setSearchResults(null);
      setSelectedId(null);
      setPath(next);
      const cached = cacheRef.current.get(next);
      if (cached) {
        setNodes(cached);
        setLoadingDir(false);
      } else {
        setNodes([]);
        setLoadingDir(true);
        setFetchingFolder(true);
      }
      void loadDirectory(next);
      return;
    }
    if (isImageNode(node)) {
      void openImagePreview(node);
    }
  };

  const prefetchNode = (node: NodeInfo) => {
    if (!node.is_directory) return;
    if (cacheRef.current.has(node.virtual_path)) return;
    void api
      .ensureDirectory(node.virtual_path)
      .then(() => api.listDirectory(node.virtual_path))
      .then((rows) => {
        cacheRef.current.set(node.virtual_path, rows);
      })
      .catch(() => undefined);
  };

  const goUp = () => {
    if (path === "\\") return;
    const parts = path.split("\\").filter(Boolean);
    parts.pop();
    const next = parts.length ? `\\${parts.join("\\")}` : "\\";
    setPath(next);
    const cached = cacheRef.current.get(next);
    if (cached) setNodes(cached);
    void loadDirectory(next);
  };

  return (
    <div className="home-page relative flex h-full min-h-0 flex-col">
      {(dragOver || uploading) && (
        <div
          className={`absolute inset-0 z-40 flex items-center justify-center bg-[#f4f4f4]/85 backdrop-blur-[2px] ${
            uploading ? "pointer-events-auto" : "pointer-events-none"
          }`}
        >
          <div className="rounded-2xl border border-dashed border-[#bbb] bg-white px-8 py-6 text-center shadow-sm">
            <p className="text-[16px] font-semibold text-[#1a1a1a]">
              {uploading ? "Uploading…" : "Drop to upload"}
            </p>
            <p className="mt-1 text-[13px] text-[#888]">
              {uploading
                ? "Sending to Google Drive"
                : path === "\\"
                  ? "Files land in your cloud (by Combine cloud setting)"
                  : `Into ${path}`}
            </p>
            {uploading && (
              <button
                type="button"
                onClick={() => {
                  void api.cancelUploadAccountPick();
                  setUploading(false);
                  setLoadError(null);
                }}
                className="mt-4 rounded-full border border-[#ddd] bg-white px-4 py-1.5 text-[13px] font-medium text-[#333] transition hover:bg-[#f5f5f5]"
              >
                Cancel
              </button>
            )}
          </div>
        </div>
      )}
      <div className="flex min-h-0 flex-1 flex-col px-8 pb-6 pt-6">
        <div
          className="home-anim mb-1 flex shrink-0 items-end justify-between gap-4"
          style={{ animationDelay: "40ms" }}
        >
          <div>
            <h1 className="text-[32px] font-semibold tracking-tight text-[#111]">{title}</h1>
            <p className="mt-0.5 flex min-h-[20px] items-center gap-2 text-[13px] text-[#8a8a8e]">
              <span>
                {syncing
                  ? "Indexing…"
                  : searching
                    ? "Searching…"
                    : isSearching
                      ? `${displayNodes.length.toLocaleString()} results`
                      : `${displayNodes.length.toLocaleString()} ${displayNodes.length === 1 ? "item" : "items"}`}
                {!isSearching && path !== "\\" ? ` · ${path}` : ""}
              </span>
              {showFetching && !syncing && (
                <span
                  className="inline-block h-3 w-3 shrink-0 animate-spin rounded-full border-[1.5px] border-[#c8c8cc] border-t-[#555]"
                  aria-label="Fetching folder"
                  title="Fetching folder"
                />
              )}
            </p>
          </div>
          {path !== "\\" && !isSearching && (
            <button
              type="button"
              onClick={goUp}
              className="rounded-full border border-[#e4e4e7] px-3 py-1.5 text-[12px] text-[#444] transition active:scale-[0.96] hover:bg-[#f4f4f5]"
            >
              Up
            </button>
          )}
        </div>

        <div
          className="home-anim mt-4 flex shrink-0 items-center justify-between gap-6"
          style={{ animationDelay: "90ms" }}
        >
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 text-[14px] font-medium leading-none text-[#333]">
              <StorageIcon className="h-[14px] w-[14px] shrink-0 text-[#333]" />
              <span>Storage</span>
            </div>
            <p className="mt-2.5 text-[13px] font-medium leading-none text-[#1a1a1a]">
              {storageUsage}
            </p>
            <div className="mt-2 flex max-w-md items-center gap-3">
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-[#ececec]">
                <div
                  className="h-full rounded-full bg-black transition-[width] duration-500"
                  style={{ width: `${storagePercent}%` }}
                />
              </div>
              <div
                className="relative flex h-10 w-10 shrink-0 items-center justify-center"
                aria-label={`${storagePercentLabel} used`}
              >
                <div
                  className="pointer-events-none absolute inset-0 rounded-full"
                  style={{
                    background: `conic-gradient(#1a1a1a ${storageDeg}deg, #d4d4d4 ${storageDeg}deg)`,
                    WebkitMask:
                      "radial-gradient(farthest-side, transparent calc(100% - 3px), #000 calc(100% - 2.5px))",
                    mask: "radial-gradient(farthest-side, transparent calc(100% - 3px), #000 calc(100% - 2.5px))",
                  }}
                />
                <span
                  className="relative z-10 text-[11px] font-semibold leading-none tabular-nums tracking-tight text-[#111]"
                  style={{ WebkitFontSmoothing: "antialiased", textRendering: "geometricPrecision" }}
                >
                  {storagePercentLabel}
                </span>
              </div>
            </div>
          </div>
          <button
            type="button"
            onClick={() => {
              setAccountsOpen(true);
              void onRefreshQuotas?.();
            }}
            className="shrink-0 self-center rounded-full bg-black px-4 py-2 text-[13px] font-medium text-white transition hover:bg-[#222] active:scale-[0.96]"
          >
            View connected account
          </button>
        </div>

        {syncing && (
          <div
            className="home-anim mt-4 shrink-0 rounded-2xl border border-[#ececee] bg-[#fafafa] px-4 py-3"
            style={{ animationDelay: "120ms" }}
          >
            <div className="mb-2 flex justify-between text-[12px] text-[#666]">
              <span>{syncProgress?.message || "Syncing metadata…"}</span>
              <span className="tabular-nums">{Math.round(syncProgress?.percent ?? 0)}%</span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-[#e8e8ea]">
              <div
                className="h-full rounded-full bg-[#111] transition-[width] duration-300"
                style={{ width: `${Math.min(100, Math.max(0, syncProgress?.percent ?? 0))}%` }}
              />
            </div>
          </div>
        )}

        <div
          className="home-anim mt-5 flex shrink-0 flex-wrap items-center gap-3"
          style={{ animationDelay: "140ms" }}
        >
          <div className="relative min-w-[220px] flex-1 max-w-md">
            <svg
              className="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-[#9a9a9e]"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              aria-hidden
            >
              <circle cx="11" cy="11" r="7" />
              <path d="M20 20l-3.5-3.5" strokeLinecap="round" />
            </svg>
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search files or folders…"
              autoComplete="off"
              spellCheck={false}
              className="home-search-input w-full rounded-full border border-[#e4e4e7] bg-white py-2.5 pl-10 pr-9 text-[13px] text-[#1a1a1a] outline-none transition placeholder:text-[#a8a8ae] focus:border-[#c8c8cc] focus:shadow-[0_0_0_3px_rgba(0,0,0,0.06)]"
            />
            {search && (
              <button
                type="button"
                onClick={() => setSearch("")}
                className="absolute right-2.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-full text-[#888] transition hover:bg-[#f0f0f2] active:scale-90"
                aria-label="Clear search"
              >
                ×
              </button>
            )}
          </div>

          <button
            type="button"
            disabled={busy && !signingIn}
            onClick={() => void handleImport()}
            className="inline-flex h-9 items-center justify-center gap-1.5 rounded-full border border-[#e4e4e7] bg-white px-3.5 text-[13px] font-medium leading-none text-[#333] transition hover:bg-[#f7f7f8] active:scale-[0.96] disabled:opacity-50"
          >
            {!signingIn && <PlusIcon className="shrink-0 text-[#333]" />}
            <span className="leading-none">{signingIn ? "Cancel sign-in" : "Import"}</span>
          </button>

          <div className="ml-auto flex items-center gap-2">
            <div className="flex items-center rounded-lg border border-[#e4e4e7] p-0.5">
              <button
                type="button"
                title="Grid"
                onClick={() => setView("grid")}
                className={`rounded-md p-1.5 transition active:scale-90 ${view === "grid" ? "bg-[#ececee]" : "text-[#888] hover:bg-[#f4f4f5]"}`}
              >
                <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                  <rect x="2" y="2" width="5" height="5" rx="1" />
                  <rect x="9" y="2" width="5" height="5" rx="1" />
                  <rect x="2" y="9" width="5" height="5" rx="1" />
                  <rect x="9" y="9" width="5" height="5" rx="1" />
                </svg>
              </button>
              <button
                type="button"
                title="List"
                onClick={() => setView("list")}
                className={`rounded-md p-1.5 transition active:scale-90 ${view === "list" ? "bg-[#ececee]" : "text-[#888] hover:bg-[#f4f4f5]"}`}
              >
                <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                  <rect x="2" y="3" width="12" height="2" rx="0.5" />
                  <rect x="2" y="7" width="12" height="2" rx="0.5" />
                  <rect x="2" y="11" width="12" height="2" rx="0.5" />
                </svg>
              </button>
            </div>
          </div>
        </div>

        {crumbs.length > 0 && (
          <div className="mt-4 flex flex-wrap items-center gap-1 text-[12px] text-[#8a8a8e]">
            <button type="button" className="hover:text-[#111]" onClick={() => setPath("\\")}>
              {title}
            </button>
            {crumbs.map((c, i) => {
              const next = `\\${crumbs.slice(0, i + 1).join("\\")}`;
              return (
                <span key={next} className="flex items-center gap-1">
                  <span>/</span>
                  <button type="button" className="hover:text-[#111]" onClick={() => setPath(next)}>
                    {c}
                  </button>
                </span>
              );
            })}
          </div>
        )}

        <div className="mt-4 flex min-h-0 flex-1 flex-col">
          {loadError && (
            <p className="mb-3 shrink-0 text-[13px] text-[#e24b4a]">{loadError}</p>
          )}

          {isEmpty ? (
            <div className="home-anim flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
              <img
                src={EMPTY_FOLDER_SRC}
                alt=""
                className="mb-6 h-40 w-auto object-contain opacity-90 md:h-48"
                draggable={false}
              />
              <p className="text-[18px] font-semibold text-[#222]">
                {isSearching ? "No matches found" : "Drop files here"}
              </p>
              <p className="mt-2 max-w-sm text-[13px] text-[#8a8a8e]">
                {isSearching
                  ? `Nothing matched “${search.trim()}”. Try another name.`
                  : uploading
                    ? "Uploading to Google Drive…"
                    : "Drag files or folders here to upload, or import an account."}
              </p>
              {!isSearching && (
                <button
                  type="button"
                  disabled={busy && !signingIn}
                  onClick={() => void handleImport()}
                  className="mt-5 inline-flex h-9 items-center justify-center gap-1.5 rounded-full border border-[#e4e4e7] bg-white px-4 text-[13px] font-medium leading-none text-[#333] transition active:scale-[0.96] disabled:opacity-50"
                >
                  {!signingIn && <PlusIcon className="shrink-0 text-[#333]" />}
                  <span className="leading-none">{signingIn ? "Cancel sign-in" : "Import"}</span>
                </button>
              )}
            </div>
          ) : view === "list" ? (
            <div className="home-anim file-list-panel relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-[#e8e8ea] bg-white">
              <div className="grid shrink-0 grid-cols-[minmax(0,2.2fr)_1.1fr_0.9fr_0.7fr_24px] gap-2 border-b border-[#ececee] px-4 py-2.5 text-[12px] font-medium text-[#8a8a8e]">
                <span className="border-r border-[#ececee] pr-2">Name</span>
                <span className="border-r border-[#ececee] pr-2">Date modified</span>
                <span className="border-r border-[#ececee] pr-2">Type</span>
                <span className="border-r border-[#ececee] pr-2">Size</span>
                <span aria-hidden />
              </div>
              <ul className="file-list-scroll min-h-0 flex-1 overflow-y-auto overflow-x-hidden py-1 pb-12">
                {displayNodes.map((node, i) => {
                  const selected = selectedId === node.id;
                  const email = accountEmailById.get(node.account_id) ?? "Unknown account";
                  return (
                    <li
                      key={node.id}
                      className="file-row-anim"
                      style={{ animationDelay: `${Math.min(i, 18) * 18}ms` }}
                    >
                      <button
                        type="button"
                        onClick={() => {
                          setSelectedId(node.id);
                          if (node.is_directory || isImageNode(node)) openNode(node);
                        }}
                        onDoubleClick={() => openNode(node)}
                        onMouseEnter={() => prefetchNode(node)}
                        className={`grid w-full grid-cols-[minmax(0,2.2fr)_1.1fr_0.9fr_0.7fr_24px] gap-2 px-4 py-2 text-left text-[13px] text-[#222] outline-none transition active:scale-[0.995] ${
                          selected
                            ? "bg-[#f0f0f2]"
                            : "hover:bg-[#f7f7f8]"
                        }`}
                      >
                        <span className="flex min-w-0 items-center gap-2.5 truncate">
                          <FileIcon folder={node.is_directory} />
                          <span className="truncate">{node.name || "unnamed"}</span>
                        </span>
                        <span className="truncate text-[#8a8a8e]">
                          {formatModified(node.modified_at)}
                        </span>
                        <span className="truncate text-[#8a8a8e]">{fileTypeLabel(node)}</span>
                        <span className="truncate text-[#8a8a8e]">
                          {node.is_directory ? "" : formatBytes(node.size)}
                        </span>
                        <span className="flex items-center justify-end">
                          <AccountSourceInfo
                            email={email}
                            tipId={`file-src-${node.id}`}
                          />
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
              <div
                className="file-list-bottom-fade pointer-events-none absolute inset-x-0 bottom-0 z-10 h-16 rounded-b-2xl"
                aria-hidden
              />
            </div>
          ) : (
            <div className="home-anim file-list-scroll min-h-0 flex-1 overflow-y-auto">
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
                {displayNodes.map((node, i) => {
                  const selected = selectedId === node.id;
                  const email = accountEmailById.get(node.account_id) ?? "Unknown account";
                  return (
                    <button
                      key={node.id}
                      type="button"
                      onClick={() => {
                        setSelectedId(node.id);
                        if (node.is_directory || isImageNode(node)) openNode(node);
                      }}
                      onMouseEnter={() => prefetchNode(node)}
                      onDoubleClick={() => openNode(node)}
                      className={`file-row-anim relative flex flex-col items-center gap-2 overflow-visible rounded-2xl border px-3 py-4 text-center outline-none transition active:scale-[0.97] ${
                        selected
                          ? "border-[#d4d4d8] bg-[#f4f4f5]"
                          : "border-[#ececee] bg-white hover:bg-[#fafafa]"
                      }`}
                      style={{ animationDelay: `${Math.min(i, 18) * 18}ms` }}
                    >
                      <span className="absolute right-2 top-2 z-10">
                        <AccountSourceInfo
                          email={email}
                          tipId={`file-src-grid-${node.id}`}
                        />
                      </span>
                      <FileIcon folder={node.is_directory} />
                      <span className="line-clamp-2 w-full text-[12px] text-[#222]">
                        {node.name || "unnamed"}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>

      {(preview || previewBusy) && (
        <div
          className="accounts-backdrop fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6 backdrop-blur-md"
          onClick={closePreview}
        >
          <div
            className="accounts-panel flex max-h-[min(860px,92vh)] w-full max-w-3xl flex-col overflow-hidden rounded-[1.25rem] border border-[#e4e4e4] bg-white shadow-[0_24px_80px_rgba(0,0,0,0.18)]"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-4 border-b border-[#ececec] px-6 py-4">
              <div className="min-w-0">
                <h2 className="truncate text-[18px] font-semibold text-[#1a1a1a]">
                  {preview?.name || "Opening image…"}
                </h2>
                <p className="mt-1 text-[13px] text-[#8a8a8a]">
                  {preview
                    ? `${formatBytes(preview.size)} logical · On disk: ${preview.on_disk_bytes} bytes · streamed by Storbyte`
                    : "Streaming preview…"}
                </p>
                {preview && (
                  <p className="mt-0.5 truncate text-[12px] text-[#aaa]">
                    {accountEmailById.get(preview.account_id) ?? preview.account_id}
                    {preview.truncated ? " · preview truncated" : ""}
                  </p>
                )}
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  onClick={closePreview}
                  className="rounded-full border border-[#e4e4e7] px-3 py-2 text-[13px] text-[#444] transition hover:bg-[#f4f4f5] active:scale-[0.96]"
                >
                  Close
                </button>
              </div>
            </div>
            <div className="flex min-h-0 flex-1 items-center justify-center bg-[#f7f7f8] p-6">
              {previewBusy && !previewUrl ? (
                <p className="text-[13px] text-[#888]">Streaming…</p>
              ) : previewUrl ? (
                <img
                  src={previewUrl}
                  alt={preview?.name || "Preview"}
                  className="max-h-[min(640px,70vh)] max-w-full object-contain"
                  draggable={false}
                />
              ) : (
                <p className="text-[13px] text-[#888]">No preview</p>
              )}
            </div>
          </div>
        </div>
      )}

      {accountsOpen && (
        <div
          className="accounts-backdrop fixed inset-0 z-50 flex items-center justify-center bg-black/25 p-6 backdrop-blur-md"
          onClick={() => setAccountsOpen(false)}
        >
          <div
            className="accounts-panel flex max-h-[min(720px,90vh)] w-full max-w-3xl flex-col overflow-hidden rounded-[1.25rem] border border-[#e4e4e4] bg-white shadow-[0_24px_80px_rgba(0,0,0,0.18)]"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between gap-4 border-b border-[#ececec] px-6 py-4">
              <div>
                <h2 className="text-[18px] font-semibold text-[#1a1a1a]">Connected accounts</h2>
                <p className="mt-1 text-[13px] text-[#8a8a8a]">
                  Merged storage · {connectedAccounts.length}{" "}
                  {connectedAccounts.length === 1 ? "account" : "accounts"}
                </p>
              </div>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  disabled={busy && !signingIn}
                  onClick={() => void handleImport()}
                  className="rounded-full bg-black px-4 py-2 text-[13px] font-semibold text-white transition hover:bg-[#222] active:scale-[0.96] disabled:opacity-50"
                >
                  {signingIn ? "Cancel sign-in" : "+ Connect Account"}
                </button>
                <button
                  type="button"
                  onClick={() => setAccountsOpen(false)}
                  className="rounded-full border border-[#e4e4e7] px-3 py-2 text-[13px] text-[#444] transition hover:bg-[#f4f4f5] active:scale-[0.96]"
                >
                  Close
                </button>
              </div>
            </div>

            <div className="sx-scroll min-h-0 flex-1 overflow-auto px-6 py-2">
              <table className="w-full table-fixed border-collapse text-left text-[13px]">
                <colgroup>
                  <col className="w-[32%]" />
                  <col className="w-[22%]" />
                  <col className="w-[26%]" />
                  <col className="w-[12%]" />
                  <col className="w-[8%]" />
                </colgroup>
                <thead>
                  <tr className="border-b border-[#ececec] text-[12px] font-medium text-[#697386]">
                    <th className="pb-3 pr-4 font-medium">Email</th>
                    <th className="pb-3 pr-4 font-medium">Storage</th>
                    <th className="pb-3 pr-4 font-medium">Progress</th>
                    <th className="pb-3 pr-4 font-medium">Status</th>
                    <th className="pb-3 font-medium">Delete</th>
                  </tr>
                </thead>
                <tbody>
                  {connectedAccounts.length > 0 ? (
                    connectedAccounts.map((account) => {
                      const used = Math.max(0, account.usage_bytes ?? 0);
                      const total = Math.max(0, account.quota_bytes ?? 0);
                      const pct =
                        total > 0 ? Math.min(100, Math.max(0, (used / total) * 100)) : 0;
                      const connected = account.status === "connected";
                      return (
                        <tr key={account.id} className="border-b border-[#f0f0f0]">
                          <td className="max-w-0 py-4 pr-4 align-middle font-medium text-[#1a1a1a]">
                            <span className="block truncate" title={account.email}>
                              {account.email}
                            </span>
                          </td>
                          <td className="py-4 pr-4 align-middle tabular-nums text-[#425466]">
                            {total > 0
                              ? `${formatBytes(used)} / ${formatBytes(total)}`
                              : "—"}
                          </td>
                          <td className="py-4 pr-4 align-middle">
                            <div className="flex w-full items-center gap-2.5">
                              <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-[#e6e6e6]">
                                <div
                                  className="h-full rounded-full bg-black transition-[width] duration-500"
                                  style={{ width: `${pct}%` }}
                                />
                              </div>
                              <span className="w-9 shrink-0 tabular-nums text-[#6a6a6a]">
                                {`${Math.round(pct)}%`}
                              </span>
                            </div>
                          </td>
                          <td className="py-4 pr-4 align-middle">
                            {connected ? (
                              <span className="inline-flex rounded-full bg-[#e3f9e5] px-2.5 py-0.5 text-[12px] font-medium text-[#0e6245]">
                                Connected
                              </span>
                            ) : (
                              <button
                                type="button"
                                disabled={busy || signingIn}
                                onClick={() => onReconnect(account.id)}
                                className="inline-flex rounded-full bg-[#111] px-2.5 py-0.5 text-[12px] font-medium text-white transition hover:bg-[#333] disabled:opacity-50"
                              >
                                Reconnect
                              </button>
                            )}
                          </td>
                          <td className="py-4 align-middle">
                            <button
                              type="button"
                              disabled={busy}
                              onClick={() => void onRemove(account.id)}
                              className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-[#f0a8a2] bg-[#fdecea] text-[#e25950] transition hover:border-[#e25950] hover:bg-[#fadad6] disabled:opacity-50"
                              aria-label={`Delete ${account.email}`}
                              title="Delete"
                            >
                              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden>
                                <path
                                  d="M3.5 4.5h9M6.5 4.5V3.5h3v1M5.5 4.5l.5 8h4l.5-8"
                                  stroke="currentColor"
                                  strokeWidth="1.3"
                                  strokeLinecap="round"
                                  strokeLinejoin="round"
                                />
                              </svg>
                            </button>
                          </td>
                        </tr>
                      );
                    })
                  ) : (
                    <tr>
                      <td colSpan={5} className="py-10 text-center text-[14px] text-[#8a8a8a]">
                        No account connected
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>

            <div className="border-t border-[#ececec] px-6 py-3 text-[12px] text-[#8a8a8a]">
              Total merged · {storageUsage} · {storagePercentLabel} used
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
