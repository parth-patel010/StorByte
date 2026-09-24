import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import * as api from "../services/api";
import type { AccountInfo, AppSettings, UploadPlacement } from "../types";

type TabId = "general" | "drive" | "billing" | "updates";

type Props = {
  open: boolean;
  onClose: () => void;
  settings: AppSettings;
  accounts: AccountInfo[];
  letters: string[];
  busy: boolean;
  onChange: (patch: Partial<AppSettings>) => void;
  onAddAccount: () => void | Promise<unknown>;
  onSignOut: (accountId: string) => void;
};

const cacheOptions = [
  { label: "Disabled (0)", value: 0 },
  { label: "256 MB", value: 256 * 1024 * 1024 },
  { label: "512 MB", value: 512 * 1024 * 1024 },
  { label: "1 GB", value: 1024 * 1024 * 1024 },
  { label: "5 GB", value: 5 * 1024 * 1024 * 1024 },
  { label: "10 GB", value: 10 * 1024 * 1024 * 1024 },
];

const placementOptions: { value: UploadPlacement; label: string }[] = [
  { value: "most_free", label: "Highest available space drive" },
  { value: "least_free", label: "Less space available drive" },
  { value: "ask", label: "Ask every time" },
];

const tabs: { id: TabId; label: string; icon: React.ReactNode }[] = [
  {
    id: "general",
    label: "General",
    icon: (
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" />
        <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9c.2.6.7 1 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
      </svg>
    ),
  },
  {
    id: "drive",
    label: "Virtual Drive",
    icon: (
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <rect x="3" y="5" width="18" height="14" rx="2" />
        <path d="M3 10h18" />
      </svg>
    ),
  },
  {
    id: "billing",
    label: "Usage & Billing",
    icon: (
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <path d="M4 7h16v10H4V7Z" />
        <path d="M4 11h16" />
      </svg>
    ),
  },
  {
    id: "updates",
    label: "Updates",
    icon: (
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <path d="M4 12a8 8 0 0 1 14-5.3M20 12a8 8 0 0 1-14 5.3" strokeLinecap="round" />
        <path d="M18 4v4h-4M6 20v-4h4" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    ),
  },
];

function displayNameFromEmail(email: string) {
  const local = email.split("@")[0] || email;
  return local
    .replace(/[._-]+/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden>
      <path
        d="M3.5 8.5 6.5 11.5 12.5 4.5"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ChevronIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden>
      <path
        d="M3 4.5 6 7.5 9 4.5"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

type MenuOption<T extends string | number> = { value: T; label: string };

/** Custom pill + floating checkmarked menu (no native OS select). */
function SettingsMenu<T extends string | number>({
  value,
  options,
  disabled,
  onChange,
  menuWidth = 220,
  maxHeight = 220,
  compact,
}: {
  value: T;
  options: MenuOption<T>[];
  disabled?: boolean;
  onChange: (v: T) => void;
  menuWidth?: number;
  maxHeight?: number;
  /** Narrow trigger (e.g. drive letter V:) */
  compact?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [coords, setCoords] = useState<{ top: number; left: number; openUp: boolean } | null>(
    null,
  );
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const label = options.find((o) => o.value === value)?.label ?? String(value);

  const placeMenu = () => {
    const btn = btnRef.current;
    if (!btn) return;
    const r = btn.getBoundingClientRect();
    const gap = 6;
    const spaceBelow = window.innerHeight - r.bottom - 12;
    const spaceAbove = r.top - 12;
    const openUp = spaceBelow < Math.min(maxHeight, 160) && spaceAbove > spaceBelow;
    const left = Math.min(
      Math.max(8, r.right - menuWidth),
      window.innerWidth - menuWidth - 8,
    );
    setCoords({
      top: openUp ? r.top - gap : r.bottom + gap,
      left,
      openUp,
    });
  };

  useLayoutEffect(() => {
    if (!open) {
      setCoords(null);
      return;
    }
    placeMenu();
    const onWin = () => placeMenu();
    window.addEventListener("resize", onWin);
    window.addEventListener("scroll", onWin, true);
    return () => {
      window.removeEventListener("resize", onWin);
      window.removeEventListener("scroll", onWin, true);
    };
  }, [open, maxHeight, menuWidth]);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      const t = e.target as Node;
      if (btnRef.current?.contains(t) || menuRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="relative shrink-0">
      <button
        ref={btnRef}
        type="button"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-1.5 rounded-full border border-[#ddd] bg-white text-left text-[13px] text-[#1a1a1a] shadow-[0_1px_0_rgba(0,0,0,0.02)] transition hover:border-[#ccc] hover:bg-[#fafafa] disabled:opacity-50 ${
          compact ? "px-2.5 py-1.5" : "px-3 py-1.5"
        } ${open ? "border-[#bbb] bg-[#fafafa]" : ""}`}
      >
        <span className={`truncate ${compact ? "max-w-[3rem]" : "max-w-[200px]"}`}>{label}</span>
        <ChevronIcon />
      </button>
      {open &&
        coords &&
        createPortal(
          <div
            ref={menuRef}
            style={{
              position: "fixed",
              top: coords.openUp ? undefined : coords.top,
              bottom: coords.openUp ? window.innerHeight - coords.top : undefined,
              left: coords.left,
              width: menuWidth,
              maxHeight,
              zIndex: 200,
            }}
            className="sx-scroll overflow-y-auto rounded-xl border border-[#e6e6e6] bg-white py-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.16)]"
          >
            {options.map((opt) => {
              const selected = opt.value === value;
              return (
                <button
                  key={String(opt.value)}
                  type="button"
                  onClick={() => {
                    onChange(opt.value);
                    setOpen(false);
                  }}
                  className={`flex w-full items-center gap-2 border-0 px-3 py-2 text-left text-[13px] transition ${
                    selected
                      ? "bg-[#f0f0f0] font-medium text-[#1a1a1a]"
                      : "bg-transparent text-[#1a1a1a] hover:bg-[#f5f5f5]"
                  }`}
                >
                  <span
                    className={`flex w-4 shrink-0 justify-center ${
                      selected ? "text-[#1a1a1a]" : "text-transparent"
                    }`}
                  >
                    <CheckIcon />
                  </span>
                  <span>{opt.label}</span>
                </button>
              );
            })}
          </div>,
          document.body,
        )}
    </div>
  );
}

function RowCard({ children }: { children: React.ReactNode }) {
  return <div className="overflow-hidden rounded-2xl bg-[#f3f3f3]">{children}</div>;
}

function Row({
  children,
  last,
}: {
  children: React.ReactNode;
  last?: boolean;
}) {
  return (
    <div
      className={`flex items-center gap-3 px-4 py-3.5 ${
        last ? "" : "border-b border-[#e6e6e6]"
      }`}
    >
      {children}
    </div>
  );
}

export function SettingsModal({
  open,
  onClose,
  settings,
  accounts,
  letters,
  busy,
  onChange,
  onAddAccount,
  onSignOut,
}: Props) {
  const [tab, setTab] = useState<TabId>("general");
  const [signingIn, setSigningIn] = useState(false);
  const primary = useMemo(
    () => accounts.find((a) => a.status === "connected") ?? null,
    [accounts],
  );
  const connected = useMemo(
    () => accounts.filter((a) => a.status === "connected"),
    [accounts],
  );

  const driveLetterOptions = useMemo(() => {
    const ordered = [
      settings.drive_letter,
      ...letters.filter((l) => l !== settings.drive_letter),
    ];
    return ordered.map((letter) => ({ value: letter, label: `${letter}:` }));
  }, [letters, settings.drive_letter]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const title =
    tab === "general"
      ? "General"
      : tab === "drive"
        ? "Virtual Drive"
        : tab === "billing"
          ? "Usage & Billing"
          : "Updates";

  const name = primary ? displayNameFromEmail(primary.email) : "Storbyte";
  const initial = (primary?.email?.[0] ?? "S").toUpperCase();

  const driveLetterControl = (
    <SettingsMenu
      value={settings.drive_letter}
      options={driveLetterOptions}
      disabled={busy}
      onChange={(drive_letter) => onChange({ drive_letter })}
      menuWidth={96}
      maxHeight={240}
      compact
    />
  );

  const cacheControl = (
    <SettingsMenu
      value={settings.cache_bytes}
      options={cacheOptions}
      disabled={busy}
      onChange={(cache_bytes) => onChange({ cache_bytes })}
      menuWidth={148}
      maxHeight={260}
    />
  );

  return (
    <div
      className="accounts-backdrop fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-6 backdrop-blur-md"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="accounts-panel flex h-[min(640px,88vh)] w-full max-w-[820px] overflow-hidden rounded-[1.35rem] border border-[#e8e8e8] bg-white shadow-[0_24px_80px_rgba(0,0,0,0.2)]"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
      >
        <aside className="flex w-[200px] shrink-0 flex-col bg-[#f4f4f4] p-3">
          <p className="mb-3 px-2 pt-1 text-[13px] font-semibold text-[#1a1a1a]">Settings</p>
          <nav className="flex flex-col gap-0.5">
            {tabs.map((t) => {
              const active = tab === t.id;
              return (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => setTab(t.id)}
                  className={`flex items-center gap-2.5 rounded-xl border-0 px-2.5 py-2 text-left text-[13px] transition ${
                    active
                      ? "bg-[#e4e4e4] font-medium text-[#1a1a1a]"
                      : "bg-transparent text-[#444] hover:bg-[#ececec]"
                  }`}
                >
                  <span className="text-[#555]">{t.icon}</span>
                  {t.label}
                </button>
              );
            })}
          </nav>
        </aside>

        <div className="relative flex min-w-0 flex-1 flex-col bg-[#fafafa]">
          <header className="flex items-center justify-between border-b border-[#eee] px-6 py-4">
            <h2 className="text-[17px] font-semibold tracking-tight text-[#1a1a1a]">{title}</h2>
            <button
              type="button"
              aria-label="Close settings"
              onClick={onClose}
              className="flex h-8 w-8 items-center justify-center rounded-lg border-0 bg-transparent text-[#666] transition hover:bg-[#eee] hover:text-[#111]"
            >
              <svg width="14" height="14" viewBox="0 0 12 12" aria-hidden>
                <path
                  d="M3 3l6 6M9 3 3 9"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            </button>
          </header>

          <div className="sx-scroll min-h-0 flex-1 overflow-y-auto px-6 py-5">
            {tab === "general" && (
              <div className="space-y-6">
                <section>
                  <p className="mb-2 text-[12px] font-medium text-[#8a8a8a]">Account</p>
                  <RowCard>
                    {primary ? (
                      <Row>
                        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-[#1a1a1a] text-[14px] font-semibold text-white">
                          {initial}
                        </div>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-[14px] font-medium text-[#1a1a1a]">{name}</p>
                          <p className="truncate text-[12px] text-[#777]">{primary.email}</p>
                          <p className="mt-0.5 text-[12px] text-[#999]">Plan expires: 2027</p>
                        </div>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => onSignOut(primary.id)}
                          className="shrink-0 rounded-lg border border-[#ddd] bg-white px-3 py-1.5 text-[12px] font-medium text-[#333] transition hover:bg-[#f7f7f7] disabled:opacity-50"
                        >
                          Sign Out
                        </button>
                      </Row>
                    ) : (
                      <Row last>
                        <p className="text-[13px] text-[#777]">No account connected</p>
                      </Row>
                    )}
                  </RowCard>
                  <button
                    type="button"
                    onClick={() => {
                      if (signingIn) {
                        void api.cancelGoogleOauth();
                        setSigningIn(false);
                        return;
                      }
                      setSigningIn(true);
                      void Promise.resolve(onAddAccount())
                        .catch(() => undefined)
                        .finally(() => setSigningIn(false));
                    }}
                    className="mt-2 w-full rounded-2xl border border-dashed border-[#d4d4d4] bg-transparent px-4 py-3 text-left text-[13px] font-medium text-[#333] transition hover:bg-[#f3f3f3]"
                  >
                    {signingIn ? "Cancel sign-in" : "+ Add account"}
                  </button>
                  {connected.length > 1 && (
                    <p className="mt-2 text-[12px] text-[#999]">
                      {connected.length} accounts connected
                    </p>
                  )}
                </section>

                <section>
                  <p className="mb-1 text-[13px] font-medium text-[#5c5c5c]">Combine cloud</p>
                  <p className="mb-2 text-[12px] leading-relaxed text-[#999]">
                    Choose where new files land when you drop them onto the virtual drive root
                    (Storbyte window or Explorer). Ask every time opens an account picker.
                  </p>
                  <RowCard>
                    <Row last>
                      <div className="min-w-0 flex-1">
                        <p className="text-[14px] font-medium text-[#1a1a1a]">Upload destination</p>
                        <p className="text-[12px] text-[#888]">Applies to drops on the drive root</p>
                      </div>
                      <SettingsMenu
                        value={settings.upload_placement ?? "most_free"}
                        options={placementOptions}
                        disabled={busy}
                        onChange={(upload_placement) => onChange({ upload_placement })}
                        menuWidth={268}
                        maxHeight={200}
                      />
                    </Row>
                  </RowCard>
                </section>

                <section>
                  <p className="mb-2 text-[12px] font-medium text-[#8a8a8a]">System</p>
                  <RowCard>
                    <Row>
                      <div className="min-w-0 flex-1">
                        <p className="text-[14px] font-medium text-[#1a1a1a]">Drive letter</p>
                      </div>
                      {driveLetterControl}
                    </Row>
                    <Row>
                      <div className="min-w-0 flex-1">
                        <p className="text-[14px] font-medium text-[#1a1a1a]">Zero-bit export</p>
                        <p className="text-[12px] text-[#888]">Copy stubs instead of full downloads</p>
                      </div>
                      <button
                        type="button"
                        role="switch"
                        aria-checked={settings.zero_bit_export}
                        disabled={busy}
                        onClick={() => onChange({ zero_bit_export: !settings.zero_bit_export })}
                        className={`relative h-6 w-11 shrink-0 rounded-full border-0 transition ${
                          settings.zero_bit_export ? "bg-[#1a1a1a]" : "bg-[#ccc]"
                        }`}
                      >
                        <span
                          className={`absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-white transition ${
                            settings.zero_bit_export ? "translate-x-5" : ""
                          }`}
                        />
                      </button>
                    </Row>
                    <Row last>
                      <div className="min-w-0 flex-1">
                        <p className="text-[14px] font-medium text-[#1a1a1a]">Cache size</p>
                      </div>
                      {cacheControl}
                    </Row>
                  </RowCard>
                </section>
              </div>
            )}

            {tab === "drive" && (
              <div className="space-y-4">
                <p className="text-[13px] leading-relaxed text-[#777]">
                  Mount Storbyte as a Windows drive letter. Files stream on demand — nothing is fully
                  downloaded until you open or copy them.
                </p>
                <RowCard>
                  <Row>
                    <div className="min-w-0 flex-1">
                      <p className="text-[14px] font-medium text-[#1a1a1a]">Drive letter</p>
                      <p className="text-[12px] text-[#888]">Letter used when mounting</p>
                    </div>
                    {driveLetterControl}
                  </Row>
                  <Row>
                    <div className="min-w-0 flex-1">
                      <p className="text-[14px] font-medium text-[#1a1a1a]">Launch at Windows startup</p>
                      <p className="text-[12px] text-[#888]">
                        Open Storbyte in the tray when you sign in
                      </p>
                    </div>
                    <button
                      type="button"
                      role="switch"
                      aria-checked={settings.launch_at_startup !== false}
                      disabled={busy}
                      onClick={() =>
                        onChange({ launch_at_startup: !(settings.launch_at_startup !== false) })
                      }
                      className={`relative h-6 w-11 shrink-0 rounded-full border-0 transition ${
                        settings.launch_at_startup !== false ? "bg-[#1a1a1a]" : "bg-[#ccc]"
                      }`}
                    >
                      <span
                        className={`absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-white transition ${
                          settings.launch_at_startup !== false ? "translate-x-5" : ""
                        }`}
                      />
                    </button>
                  </Row>
                  <Row>
                    <div className="min-w-0 flex-1">
                      <p className="text-[14px] font-medium text-[#1a1a1a]">Auto-mount when online</p>
                      <p className="text-[12px] text-[#888]">
                        Wait for internet, then mount the drive automatically
                      </p>
                    </div>
                    <button
                      type="button"
                      role="switch"
                      aria-checked={settings.auto_mount_on_launch !== false}
                      disabled={busy}
                      onClick={() =>
                        onChange({
                          auto_mount_on_launch: !(settings.auto_mount_on_launch !== false),
                        })
                      }
                      className={`relative h-6 w-11 shrink-0 rounded-full border-0 transition ${
                        settings.auto_mount_on_launch !== false ? "bg-[#1a1a1a]" : "bg-[#ccc]"
                      }`}
                    >
                      <span
                        className={`absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-white transition ${
                          settings.auto_mount_on_launch !== false ? "translate-x-5" : ""
                        }`}
                      />
                    </button>
                  </Row>
                  <Row last>
                    <div className="min-w-0 flex-1">
                      <p className="text-[14px] font-medium text-[#1a1a1a]">Cache size</p>
                      <p className="text-[12px] text-[#888]">RAM working set for streaming</p>
                    </div>
                    {cacheControl}
                  </Row>
                </RowCard>
              </div>
            )}

            {tab === "billing" && (
              <div className="space-y-3">
                <RowCard>
                  <Row last>
                    <div>
                      <p className="text-[14px] font-medium text-[#1a1a1a]">Storbyte Plan</p>
                      <p className="mt-1 text-[13px] text-[#777]">Plan expires: 2027</p>
                    </div>
                  </Row>
                </RowCard>
                <p className="text-[12px] text-[#999]">Billing management coming soon.</p>
              </div>
            )}

            {tab === "updates" && (
              <div className="space-y-3">
                <RowCard>
                  <Row last>
                    <div>
                      <p className="text-[14px] font-medium text-[#1a1a1a]">You&apos;re up to date</p>
                      <p className="mt-1 text-[13px] text-[#777]">
                        Storbyte Phase 1 · check back for updates.
                      </p>
                    </div>
                  </Row>
                </RowCard>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
