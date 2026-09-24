import { useEffect, useMemo, useState } from "react";
import logoMark from "../assets/stroex-logo-light.png";
import * as api from "../services/api";
import type { AccountInfo } from "../types";

type Props = {
  email: string;
  onBack: () => void;
  onContinue: () => void;
};

function formatBytes(n: number) {
  if (!Number.isFinite(n) || n < 0) return "0 Bytes";
  if (n < 1024) return `${Math.round(n)} Bytes`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n;
  let i = -1;
  do {
    v /= 1024;
    i += 1;
  } while (v >= 1024 && i < units.length - 1);
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

function accountUsed(a: AccountInfo) {
  return Math.max(0, a.usage_bytes ?? 0);
}

function accountTotal(a: AccountInfo) {
  return Math.max(0, a.quota_bytes ?? 0);
}

function accountPercent(a: AccountInfo) {
  const total = accountTotal(a);
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, (accountUsed(a) / total) * 100));
}

function GoogleDriveGlyph({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      aria-hidden
      fill="none"
    >
      <path d="M8.1 3.5h7.8L22 14.2h-7.8L8.1 3.5Z" fill="#2684FC" />
      <path d="M2 14.2 8.1 3.5l3.9 6.75L5.9 21 2 14.2Z" fill="#00AC47" />
      <path d="M5.9 21h12.2L22 14.2H9.8L5.9 21Z" fill="#FFBA00" />
    </svg>
  );
}

export function IntroStepConnect({ email: _loginEmail, onBack, onContinue }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<AccountInfo[]>([]);

  const refreshAccounts = async () => {
    const list = await api.listAccounts();
    setAccounts(list);
  };

  useEffect(() => {
    void refreshAccounts().catch(() => undefined);
  }, []);

  const connectedAccounts = useMemo(
    () => accounts.filter((a) => a.status === "connected"),
    [accounts],
  );

  const headerUsed = useMemo(
    () => connectedAccounts.reduce((sum, a) => sum + accountUsed(a), 0),
    [connectedAccounts],
  );
  const headerTotal = useMemo(
    () => connectedAccounts.reduce((sum, a) => sum + accountTotal(a), 0),
    [connectedAccounts],
  );
  const headerPercent =
    headerTotal > 0
      ? Math.min(100, Math.max(0, (headerUsed / headerTotal) * 100))
      : 0;
  const headerPercentLabel = `${Math.round(headerPercent)}%`;
  const headerUsage =
    connectedAccounts.length > 0
      ? `${formatBytes(headerUsed)} / ${formatBytes(headerTotal || 0)}`
      : "0 Bytes / 0 Bytes";
  const headerDeg = (headerPercent / 100) * 360;

  const connectAccount = async () => {
    if (busy) {
      await api.cancelGoogleOauth();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.connectGoogleAccount();
      await refreshAccounts();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (!/sign-in cancelled/i.test(msg) && !/sign-in timed out/i.test(msg)) {
        setError(msg);
      }
    } finally {
      setBusy(false);
    }
  };

  const removeAccount = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await api.removeAccount(id);
      await refreshAccounts();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const canContinue = connectedAccounts.length > 0;

  return (
    <div className="flex h-full overflow-hidden bg-[#f4f4f4] font-[family-name:var(--font-intro)] text-[#1a1a1a]">
      {/* Sidebar */}
      <aside className="flex w-[240px] shrink-0 flex-col border-r border-[#e8e8e8] bg-[#f4f4f4] px-5 py-6">
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-2.5 self-start border-0 bg-transparent p-0"
        >
          <img src={logoMark} alt="" className="h-8 w-8 object-contain" />
          <span className="text-[18px] font-semibold tracking-tight">Storbyte</span>
        </button>

        <p className="mt-10 px-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-[#9a9a9a]">
          Connector
        </p>

        <div className="relative mt-3 flex items-center gap-3 rounded-xl bg-[#ebebeb] px-3 py-2.5">
          <span className="absolute inset-y-2 left-0 w-[3px] rounded-full bg-black" />
          <GoogleDriveGlyph className="h-5 w-5 shrink-0" />
          <span className="min-w-0 flex-1 truncate text-[14px] font-medium">
            Google Drive
          </span>
          <span className="text-[16px] text-[#8a8a8a]" aria-hidden>
            ›
          </span>
        </div>

        <div className="mt-auto border-t border-[#d8d8d8] px-1 pb-1 pt-4">
          <div className="flex items-center gap-2.5">
            <StorageIcon className="h-4 w-4 shrink-0" />
            <div className="min-w-0 flex-1">
              <p className="text-[13px] font-semibold leading-none text-[#1a1a1a]">
                Storage
              </p>
              <p className="mt-1.5 text-[12px] font-medium leading-none text-[#1a1a1a]">
                {headerUsage}
              </p>
              <div className="mt-2 h-[5px] w-full overflow-hidden rounded-full bg-[#d4d4d4]">
                <div
                  className="h-full rounded-full bg-black transition-[width] duration-500"
                  style={{ width: `${headerPercent}%` }}
                />
              </div>
            </div>
            <div
              className="relative flex h-10 w-10 shrink-0 items-center justify-center"
              aria-label={`${headerPercentLabel} used`}
            >
              <div
                className="absolute inset-0 rounded-full"
                style={{
                  background: `conic-gradient(#1a1a1a ${headerDeg}deg, #d4d4d4 ${headerDeg}deg)`,
                  WebkitMask:
                    "radial-gradient(farthest-side, transparent calc(100% - 2.5px), #000 calc(100% - 2px))",
                  mask: "radial-gradient(farthest-side, transparent calc(100% - 2.5px), #000 calc(100% - 2px))",
                }}
              />
              <span className="relative text-[10px] font-bold tabular-nums text-[#1a1a1a]">
                {headerPercentLabel}
              </span>
            </div>
          </div>
        </div>
      </aside>

      {/* Main */}
      <div className="connect-main relative m-4 ml-0 flex min-w-0 flex-1 flex-col overflow-visible rounded-[1.25rem] border border-[#e4e4e4] bg-white">
        <div className="flex items-start justify-between gap-6 px-8 pb-2 pt-7">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 text-[14px] font-medium leading-none text-[#333]">
              <StorageIcon className="h-[14px] w-[14px] shrink-0 text-[#333]" />
              <span>Storage</span>
              <span className="storage-info relative ml-0.5 inline-flex items-center">
                <button
                  type="button"
                  className="inline-flex h-[14px] w-[14px] items-center justify-center rounded-full border border-[#c8c8c8] bg-white p-0 text-[9px] font-semibold leading-none text-[#8a8a8a] outline-none hover:border-[#aaa] hover:text-[#555] focus-visible:border-[#888]"
                  aria-label="Connected cloud storage"
                  aria-describedby="storage-info-tip"
                >
                  i
                </button>
                <span
                  id="storage-info-tip"
                  role="tooltip"
                  className="storage-info-tip pointer-events-none absolute left-[calc(100%+10px)] top-1/2 z-30 w-max -translate-y-1/2 rounded-md bg-[#2a2a2a] px-2.5 py-1.5 text-[12px] font-medium leading-none text-white opacity-0 shadow-md transition-opacity duration-150"
                >
                  Connected cloud storage
                  <span
                    className="absolute right-full top-1/2 h-0 w-0 -translate-y-1/2 border-y-[5px] border-r-[6px] border-y-transparent border-r-[#2a2a2a]"
                    aria-hidden
                  />
                </span>
              </span>
            </div>
            <p className="mt-2.5 text-[13px] font-medium leading-none text-[#1a1a1a]">{headerUsage}</p>
            <div className="mt-0.5 flex max-w-md items-center gap-3">
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-[#ececec]">
                <div
                  className="h-full rounded-full bg-black transition-[width] duration-500"
                  style={{ width: `${headerPercent}%` }}
                />
              </div>
              <span className="shrink-0 text-[12px] tabular-nums text-[#8a8a8a]">
                {headerPercentLabel}
              </span>
            </div>
          </div>

          <button
            type="button"
            onClick={() => void connectAccount()}
            className="shrink-0 rounded-full bg-black px-5 py-2.5 text-[14px] font-semibold text-white transition hover:bg-[#222] active:scale-[0.97]"
          >
            {busy ? "Cancel sign-in" : "+ Connect Account"}
          </button>
        </div>

        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-b-[1.25rem] border-t border-[#ececec] px-8 pb-10 pt-2">
          <div className="w-full overflow-hidden">
            <table className="w-full table-fixed border-collapse text-left text-[13px]">
              <colgroup>
                <col className="w-[28%]" />
                <col className="w-[18%]" />
                <col className="w-[26%]" />
                <col className="w-[16%]" />
                <col className="w-[12%]" />
              </colgroup>
              <thead>
                <tr className="border-b border-[#ececec] text-[12px] font-medium text-[#697386]">
                  <th className="pb-3 pr-4 text-left font-medium">Email</th>
                  <th className="pb-3 pr-4 text-left font-medium">Storage</th>
                  <th className="pb-3 pr-4 text-left font-medium">Progress</th>
                  <th className="pb-3 pr-4 text-left font-medium">Status</th>
                  <th className="pb-3 text-left font-medium">Delete</th>
                </tr>
              </thead>
              <tbody>
                {accounts.length > 0 ? (
                  accounts.map((account) => {
                    const used = accountUsed(account);
                    const total = accountTotal(account);
                    const pct = accountPercent(account);
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
                            <span className="inline-flex rounded-full bg-[#f2f2f2] px-2.5 py-0.5 text-[12px] font-medium text-[#4a4a4a]">
                              Not Connected
                            </span>
                          )}
                        </td>
                        <td className="py-4 align-middle">
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => void removeAccount(account.id)}
                            className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-[#f0a8a2] bg-[#fdecea] text-[#e25950] transition hover:border-[#e25950] hover:bg-[#fadad6] disabled:opacity-50"
                            aria-label={`Delete ${account.email}`}
                            title="Delete"
                          >
                            <TrashIcon />
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
          {error && (
            <p className="mt-4 max-w-md text-sm text-[#c0392b]">{error}</p>
          )}
        </div>

        <button
          type="button"
          disabled={!canContinue}
          onClick={onContinue}
          className="absolute bottom-6 right-6 z-20 rounded-full bg-black px-8 py-3.5 text-[15px] font-semibold text-white shadow-[0_8px_24px_rgba(0,0,0,0.18)] transition hover:bg-[#222] active:scale-[0.97] disabled:cursor-not-allowed disabled:bg-[#cfcfcf] disabled:text-[#8a8a8a] disabled:shadow-none"
        >
          Continue
        </button>
      </div>

      <style>{`
        .connect-main {
          animation: connectIn 520ms cubic-bezier(0.22, 1, 0.36, 1) both;
        }
        @keyframes connectIn {
          from { opacity: 0; transform: translateY(10px); }
          to { opacity: 1; transform: translateY(0); }
        }
        .storage-info:hover .storage-info-tip,
        .storage-info:focus-within .storage-info-tip {
          opacity: 1;
        }
      `}</style>
    </div>
  );
}

function TrashIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
      <path
        d="M3.5 4.5h9M6.5 4.5V3.25c0-.414.336-.75.75-.75h1.5c.414 0 .75.336.75.75V4.5M5.25 4.5l.5 8.25c.04.52.47.9.99.9h3.52c.52 0 .95-.38.99-.9l.5-8.25"
        stroke="currentColor"
        strokeWidth="1.35"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function StorageIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden
    >
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
