import { useCallback, useEffect, useState } from "react";
import logoMark from "../assets/stroex-logo-light.png";
import * as api from "../services/api";

type Props = {
  onBack: () => void;
  onContinue: () => void;
};

export function IntroStepDependencies({ onBack, onContinue }: Props) {
  const [installed, setInstalled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const ok = await api.winfspInstalled();
      setInstalled(ok);
      setError(null);
      return ok;
    } catch (e) {
      setInstalled(false);
      setError(e instanceof Error ? e.message : String(e));
      return false;
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const onFocus = () => {
      void refresh();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  const onInstall = async () => {
    setBusy(true);
    setError(null);
    setStatus("Installing dependency… Accept the Windows prompt if shown.");
    try {
      const ok = await api.installWinfsp();
      await refresh();
      if (ok) {
        setStatus("Dependency installed and ready.");
      } else {
        setError(
          "Install may still be finishing. Wait a moment, then try again.",
        );
        setStatus(null);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus(null);
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const ready = installed === true;

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-[#f8f8f8] font-[family-name:var(--font-intro)]">
      <div className="intro3-body flex min-h-0 flex-1 flex-col items-center justify-center px-10 text-center">
        <div className="relative mb-7">
          <div className="flex h-[4.5rem] w-[4.5rem] items-center justify-center rounded-[1.15rem] bg-white shadow-[0_8px_28px_rgba(0,0,0,0.08)] ring-1 ring-black/5">
            <img
              src={logoMark}
              alt=""
              className="h-10 w-10 object-contain"
              draggable={false}
            />
          </div>
          {ready && (
            <span className="absolute -bottom-1 -right-1 flex h-6 w-6 items-center justify-center rounded-full bg-black text-white shadow">
              <CheckIcon />
            </span>
          )}
        </div>

        <h1 className="text-[clamp(1.75rem,3.4vw,2.55rem)] font-bold leading-[1.15] tracking-[-0.03em] text-black">
          Install Storbyte Dependencies
        </h1>
        <p className="mt-3 max-w-[28rem] text-[15px] leading-relaxed text-[#8a8a8a] md:text-[16px]">
          This lets Storbyte appear as a drive in Explorer and keeps mounting
          &amp; prefetching fast.
        </p>

        <div className="mt-8 flex items-center gap-2.5 text-[15px] text-[#6b6b6b]">
          {installed === null ? (
            <span className="text-[#9a9a9a]">Checking…</span>
          ) : ready ? (
            <>
              <span className="flex h-6 w-6 items-center justify-center rounded-full bg-black text-white">
                <CheckIcon />
              </span>
              <span>Installed</span>
            </>
          ) : (
            <>
              <span className="flex h-6 w-6 items-center justify-center rounded-full border border-[#ccc] bg-white text-[11px] font-semibold text-[#888]">
                1
              </span>
              <span>Dependency required</span>
            </>
          )}
        </div>

        {status && !error && (
          <p className="mt-4 max-w-md text-sm text-[#555]">{status}</p>
        )}
        {error && (
          <p className="mt-4 max-w-md text-sm text-[#c0392b]">{error}</p>
        )}
      </div>

      <footer className="relative z-[2] flex shrink-0 items-center justify-between px-10 pb-8 pt-2 md:px-12">
        <button
          type="button"
          onClick={onBack}
          className="intro3-btn relative z-10 rounded-full border border-[#d8d8d8] bg-white px-7 py-3 text-[15px] font-medium text-[#333] transition duration-150 hover:bg-[#f5f5f5] active:scale-[0.94] active:bg-[#ebebeb]"
        >
          Back
        </button>

        <div className="pointer-events-none absolute inset-x-0 flex flex-col items-center gap-1.5 text-[13px] text-[#b0b0b0]">
          <span className="inline-flex h-[3px] w-14 overflow-hidden rounded-full bg-[#e5e5e5]">
            <span className="block h-full w-[60%] rounded-full bg-[#1a1a1a]" />
          </span>
          <span>3 of 5</span>
        </div>

        <div className="relative z-10 flex items-center gap-2.5">
          {ready ? (
            <button
              type="button"
              disabled
              className="intro3-btn inline-flex items-center gap-2 rounded-full border border-[#d8d8d8] bg-white px-5 py-3 text-[15px] font-medium text-[#333]"
            >
              <CheckIcon />
              Installed
            </button>
          ) : (
            <button
              type="button"
              disabled={busy || installed === null}
              onClick={() => void onInstall()}
              className="intro3-btn rounded-full border border-[#d8d8d8] bg-white px-5 py-3 text-[15px] font-medium text-[#111] transition duration-150 hover:bg-[#f5f5f5] active:scale-[0.94] disabled:opacity-50"
            >
              {busy ? "Installing…" : "Install dependency"}
            </button>
          )}
          <button
            type="button"
            onClick={onContinue}
            disabled={!ready}
            className="intro3-btn rounded-full bg-black px-8 py-3.5 text-[15px] font-semibold text-white transition duration-150 hover:bg-[#222] active:scale-[0.94] active:bg-[#111] disabled:cursor-not-allowed disabled:opacity-40"
          >
            Continue
          </button>
        </div>
      </footer>

      <style>{`
        .intro3-body {
          animation: intro3In 650ms cubic-bezier(0.22, 1, 0.36, 1) both;
        }
        .intro3-btn {
          animation: intro3Up 650ms cubic-bezier(0.22, 1, 0.36, 1) 160ms both;
        }
        .intro3-btn:active {
          animation: none;
          transform: scale(0.94);
        }
        @keyframes intro3In {
          from { opacity: 0; transform: translateY(14px); }
          to { opacity: 1; transform: translateY(0); }
        }
        @keyframes intro3Up {
          from { opacity: 0; transform: translateY(22px); }
          to { opacity: 1; transform: translateY(0); }
        }
      `}</style>
    </div>
  );
}

function CheckIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden>
      <path
        d="M2.5 6.2L4.8 8.5L9.5 3.5"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
