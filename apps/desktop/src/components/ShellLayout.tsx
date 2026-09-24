import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, Outlet, useLocation, useSearchParams } from "react-router-dom";
import logoMark from "../assets/stroex-logo-light.png";
import type { AccountInfo, DriveStatus } from "../types";

type Props = {
  children?: React.ReactNode;
  /** Dev helper: reopen onboarding when clicking the Storbyte mark */
  onReplayIntro?: () => void;
  onOpenSettings?: () => void;
  settingsOpen?: boolean;
  accountEmail?: string;
  driveCount?: number;
  accounts?: AccountInfo[];
  drive?: DriveStatus;
  busy?: boolean;
  onMount?: () => void;
  onUnmount?: () => void;
};

const MOUNT_ICON = "/intro/Homepage/mount_drive.png";

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

type LocId = "all" | "google";

type LocItem = {
  id: LocId;
  label: string;
  to: string;
  icon: React.ReactNode;
};

const PINNED_KEY = "Storbyte.pinnedLocations";

function GoogleDriveGlyph({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" aria-hidden fill="none">
      <path d="M8.1 3.5h7.8L22 14.2h-7.8L8.1 3.5Z" fill="#2684FC" />
      <path d="M2 14.2 8.1 3.5l3.9 6.75L5.9 21 2 14.2Z" fill="#00AC47" />
      <path d="M5.9 21h12.2L22 14.2H9.8L5.9 21Z" fill="#FFBA00" />
    </svg>
  );
}

function PinIcon({ filled }: { filled?: boolean }) {
  if (filled) {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
        <path d="M16 3a1 1 0 0 1 .8 1.6L14.4 9l2.1 2.1a1 1 0 0 1-.3 1.6L13 14.2V20a1 1 0 1 1-2 0v-5.8L7.8 12.7a1 1 0 0 1-.3-1.6L9.6 9 7.2 4.6A1 1 0 0 1 8 3h8Z" />
      </svg>
    );
  }
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
      <path d="M15 4.5 12.5 9l2.5 2.5-3.5 1.5V19l-1.5-1.5V13L7 11.5 9.5 9 7 4.5h8Z" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

const ALL_LOCATIONS: LocItem[] = [
  {
    id: "all",
    label: "All",
    to: "/?loc=all",
    icon: (
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
        <path d="M2 3h5v5H2V3Zm7 0h5v5H9V3ZM2 10h5v5H2v-5Zm7 0h5v5H9v-5Z" />
      </svg>
    ),
  },
  {
    id: "google",
    label: "Google Drive",
    to: "/?loc=google",
    icon: <GoogleDriveGlyph className="h-3.5 w-3.5" />,
  },
];

function readPinned(): LocId[] {
  try {
    const raw = localStorage.getItem(PINNED_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((id): id is LocId => id === "all" || id === "google");
  } catch {
    return [];
  }
}

function LocationRow({
  item,
  active,
  pinned,
  onTogglePin,
}: {
  item: LocItem;
  active: boolean;
  pinned: boolean;
  onTogglePin: (id: LocId) => void;
}) {
  return (
    <div
      className={`group relative flex items-center rounded-xl transition ${
        active ? "bg-[#ebebeb]" : "hover:bg-[#efefef]"
      }`}
    >
      <Link
        to={item.to}
        className={`relative flex min-w-0 flex-1 items-center gap-3 rounded-xl py-2.5 pl-3 pr-9 text-[14px] no-underline ${
          active ? "font-medium text-[#1a1a1a]" : "text-[#333]"
        }`}
      >
        {active && (
          <span className="absolute inset-y-2 left-0 w-[3px] rounded-full bg-black" />
        )}
        <span
          className={`flex h-5 w-5 shrink-0 items-center justify-center ${
            item.id === "google" ? "" : active ? "text-[#1a1a1a]" : "text-[#555]"
          }`}
        >
          {item.icon}
        </span>
        <span className="truncate">{item.label}</span>
      </Link>
      <button
        type="button"
        title={pinned ? "Unpin" : "Pin to top"}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onTogglePin(item.id);
        }}
        className={`absolute right-1 top-1/2 flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded-lg border-0 transition ${
          pinned
            ? "bg-transparent text-[#1a1a1a] opacity-100"
            : "bg-transparent text-[#8a8a8a] opacity-0 hover:bg-black/5 hover:text-[#1a1a1a] group-hover:opacity-100"
        }`}
      >
        <PinIcon filled={pinned} />
      </button>
    </div>
  );
}

export function ShellLayout({
  children,
  onReplayIntro,
  onOpenSettings,
  settingsOpen = false,
  accountEmail,
  driveCount = 0,
  accounts = [],
  drive,
  busy = false,
  onMount,
  onUnmount,
}: Props) {
  const loc = useLocation();
  const [params] = useSearchParams();
  const activeLoc = params.get("loc") || "all";
  const onHome = loc.pathname === "/";
  const initial = (accountEmail?.trim()?.[0] ?? "S").toUpperCase();
  const mounted = Boolean(drive?.mounted);
  const letter = (drive?.drive_letter || "V").replace(":", "").toUpperCase();
  const canToggleMount = Boolean(onMount && onUnmount) && driveCount > 0;

  const storageUsed = accounts
    .filter((a) => a.status === "connected")
    .reduce((sum, a) => sum + Math.max(0, a.usage_bytes ?? 0), 0);
  const storageTotal = accounts
    .filter((a) => a.status === "connected")
    .reduce((sum, a) => sum + Math.max(0, a.quota_bytes ?? 0), 0);
  const storageFree = Math.max(0, storageTotal - storageUsed);
  const storagePercent =
    storageTotal > 0 ? Math.min(100, Math.max(0, (storageUsed / storageTotal) * 100)) : 0;
  const barHot = storagePercent >= 90;

  const [pinnedIds, setPinnedIds] = useState<LocId[]>(() => readPinned());

  useEffect(() => {
    localStorage.setItem(PINNED_KEY, JSON.stringify(pinnedIds));
  }, [pinnedIds]);

  const togglePin = useCallback((id: LocId) => {
    setPinnedIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [id, ...prev.filter((x) => x !== id)],
    );
  }, []);

  const byId = useMemo(() => {
    const map = new Map(ALL_LOCATIONS.map((l) => [l.id, l]));
    return map;
  }, []);

  const pinnedItems = pinnedIds
    .map((id) => byId.get(id))
    .filter((x): x is LocItem => Boolean(x));

  const locationItems = ALL_LOCATIONS.filter((l) => !pinnedIds.includes(l.id));

  return (
    <div className="space-shell flex h-full min-h-0 bg-[#f4f4f4] text-[#1a1a1a]">
      <aside className="flex w-[232px] shrink-0 flex-col border-r border-[#e8e8e8] bg-[#f4f4f4] px-3 py-4">
        <button
          type="button"
          onClick={() => onReplayIntro?.()}
          title="Replay onboarding (dev)"
          className="mb-6 flex items-center gap-2 border-0 bg-transparent px-2 py-1 text-left"
        >
          <img src={logoMark} alt="" className="h-8 w-8 object-contain" draggable={false} />
          <span className="text-[15px] font-semibold tracking-tight text-[#1a1a1a]">Storbyte</span>
        </button>

        <p className="mb-2 px-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-[#9a9a9a]">
          Pinned
        </p>
        <nav className="mb-1 flex min-h-[36px] flex-col gap-0.5">
          {pinnedItems.length === 0 ? (
            <p className="px-3 py-1 text-[12px] text-[#b0b0b0]">Hover a location to pin</p>
          ) : (
            pinnedItems.map((item) => (
              <LocationRow
                key={`pin-${item.id}`}
                item={item}
                active={onHome && activeLoc === item.id}
                pinned
                onTogglePin={togglePin}
              />
            ))
          )}
        </nav>

        <p className="mb-1 mt-3 px-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-[#9a9a9a]">
          Locations
        </p>
        <nav className="flex flex-col gap-0.5">
          {locationItems.map((item) => (
            <LocationRow
              key={item.id}
              item={item}
              active={onHome && activeLoc === item.id}
              pinned={false}
              onTogglePin={togglePin}
            />
          ))}
          {locationItems.length === 0 && (
            <p className="px-3 py-1 text-[12px] text-[#b0b0b0]">All locations pinned</p>
          )}
        </nav>

        <div className="mt-auto space-y-2 px-1 pb-1">
          <div className="rounded-xl border border-[#e8e8e8] bg-white px-2.5 py-2.5">
            <div className="flex items-start gap-2.5">
              <img
                src={MOUNT_ICON}
                alt=""
                className="mt-0.5 h-9 w-9 shrink-0 object-contain"
                draggable={false}
              />
              <div className="min-w-0 flex-1">
                <p className="truncate text-[13px] font-medium leading-tight text-[#1a1a1a]">
                  Storbyte ({letter}:)
                </p>
                <div className="mt-1.5 h-[6px] w-full overflow-hidden rounded-[2px] bg-[#e5e5e5]">
                  <div
                    className={`h-full rounded-[2px] transition-[width] duration-500 ${
                      barHot ? "bg-[#e24b4a]" : "bg-[#0078d4]"
                    }`}
                    style={{
                      width: storageTotal > 0 ? `${storagePercent}%` : "0%",
                    }}
                  />
                </div>
                <p className="mt-1 truncate text-[11px] leading-tight text-[#666]">
                  {storageTotal > 0
                    ? `${formatBytes(storageFree)} free of ${formatBytes(storageTotal)}`
                    : mounted
                      ? "Mounted · streaming"
                      : drive?.waiting_for_network
                        ? "Waiting for internet…"
                        : canToggleMount
                          ? "Ready to mount"
                          : "Connect an account"}
                </p>
                {mounted && (
                  <p className="mt-0.5 text-[10px] font-medium uppercase tracking-wide text-[#0078d4]">
                    Mounted
                  </p>
                )}
                {!mounted && drive?.waiting_for_network && (
                  <p className="mt-0.5 text-[10px] font-medium uppercase tracking-wide text-[#c47b00]">
                    Waiting for network
                  </p>
                )}
              </div>
            </div>
            <button
              type="button"
              disabled={busy || !canToggleMount}
              onClick={() => {
                if (mounted) onUnmount?.();
                else onMount?.();
              }}
              className={`mt-2.5 w-full rounded-lg px-2.5 py-1.5 text-[12px] font-medium transition active:scale-[0.98] disabled:opacity-40 ${
                mounted
                  ? "border border-[#e4e4e7] bg-white text-[#333] hover:bg-[#f7f7f8]"
                  : "bg-[#111] text-white hover:bg-[#222]"
              }`}
            >
              {busy ? "Working…" : mounted ? `Unmount ${letter}:` : `Mount ${letter}:`}
            </button>
          </div>

          <div className="flex items-center gap-2 rounded-xl px-1.5 py-1.5">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-black text-[12px] font-semibold text-white">
              {initial}
            </div>
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13px] font-medium text-[#1a1a1a]">My Space</p>
              <p className="truncate text-[11px] text-[#8a8a8a]">
                {driveCount} {driveCount === 1 ? "drive" : "drives"}
              </p>
            </div>
            <button
              type="button"
              title="Settings"
              onClick={() => onOpenSettings?.()}
              className={`flex h-8 w-8 items-center justify-center rounded-lg border-0 bg-transparent text-[#666] transition hover:bg-[#efefef] ${
                settingsOpen ? "bg-[#ebebeb] text-[#1a1a1a]" : ""
              }`}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                <path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" />
                <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9c.2.6.7 1 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
              </svg>
            </button>
          </div>
        </div>
      </aside>

      <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-white">
        {children ?? <Outlet />}
      </main>
    </div>
  );
}
