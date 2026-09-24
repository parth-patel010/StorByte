import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import logoMark from "../assets/stroex-logo-light.png";
import * as api from "../services/api";
import type { SyncProgress } from "../types";

type Props = {
  onDone: () => void;
};

const VIDEO_SRC =
  "https://d8j0ntlcm91z4.cloudfront.net/user_38xzZboKViGWJOttwIXH07lWA1P/hf_20260405_170732_8a9ccda6-5cff-4628-b164-059c500a2b41.mp4";

const STATUS_LINES = [
  "Loading your Drive folders",
  "Fetching top-level files",
  "Creating your workspace",
  "Almost ready",
];

export function IntroStepFetching({ onDone }: Props) {
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  const [percent, setPercent] = useState(0);
  const [statusIndex, setStatusIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  const maxPercentRef = useRef(0);
  const lastPaintRef = useRef(0);
  const pendingRef = useRef<SyncProgress | null>(null);
  const flushTimerRef = useRef<number | null>(null);

  const paintProgress = (p: SyncProgress) => {
    const nextPercent = Math.min(100, Math.max(0, p.percent));
    // Never go backwards — only advance the bar.
    const mono = Math.max(maxPercentRef.current, nextPercent);
    maxPercentRef.current = mono;
    setPercent(mono);
  };

  const scheduleProgress = (p: SyncProgress) => {
    pendingRef.current = p;
    const now = performance.now();
    if (now - lastPaintRef.current >= 200) {
      lastPaintRef.current = now;
      paintProgress(p);
      pendingRef.current = null;
      return;
    }
    if (flushTimerRef.current !== null) return;
    flushTimerRef.current = window.setTimeout(() => {
      flushTimerRef.current = null;
      if (pendingRef.current) {
        lastPaintRef.current = performance.now();
        paintProgress(pendingRef.current);
        pendingRef.current = null;
      }
    }, 200);
  };

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const start = async () => {
      setError(null);
      maxPercentRef.current = 0;
      setPercent(0);
      setStatusIndex(0);

      unlisten = await listen<SyncProgress>("sync-progress", (event) => {
        if (cancelled) return;
        scheduleProgress(event.payload);
      });

      try {
        await api.syncMetadataShallow();
        if (cancelled) return;
        if (flushTimerRef.current !== null) {
          window.clearTimeout(flushTimerRef.current);
          flushTimerRef.current = null;
        }
        if (pendingRef.current) {
          paintProgress(pendingRef.current);
          pendingRef.current = null;
        }
        maxPercentRef.current = 100;
        setPercent(100);
        setStatusIndex(STATUS_LINES.length - 1);
        onDoneRef.current();
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      }
    };

    void start();
    return () => {
      cancelled = true;
      if (flushTimerRef.current !== null) {
        window.clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      unlisten?.();
    };
  }, [attempt]);

  // Rotate status lines with a slide-up swap while sync is running.
  useEffect(() => {
    if (error || percent >= 100) return;
    const id = window.setInterval(() => {
      setStatusIndex((i) => (i + 1) % STATUS_LINES.length);
    }, 2800);
    return () => window.clearInterval(id);
  }, [error, percent, attempt]);

  const retry = () => {
    setError(null);
    setAttempt((n) => n + 1);
  };

  return (
    <div className="relative flex h-full w-full items-center justify-center overflow-hidden bg-black text-white">
      <video
        className="absolute inset-0 h-full w-full object-cover"
        src={VIDEO_SRC}
        autoPlay
        muted
        loop
        playsInline
        preload="auto"
      />
      <div className="noise-overlay pointer-events-none absolute inset-0 opacity-[0.7] mix-blend-overlay" />
      <div className="pointer-events-none absolute inset-0 bg-gradient-to-b from-black/30 via-transparent to-black/60" />

      <div className="relative z-10 flex w-full max-w-md flex-col items-center px-8 text-center">
        <img
          src={logoMark}
          alt="Storbyte"
          className="h-14 w-14 object-contain brightness-0 invert md:h-16 md:w-16"
          draggable={false}
        />
        <p className="mt-4 text-[28px] font-semibold tracking-tight">Storbyte</p>

        <div className="status-slot mt-3 h-[22px] w-full max-w-[320px] overflow-hidden">
          <p
            key={statusIndex}
            className="status-slide text-[15px] text-white/85"
          >
            {STATUS_LINES[statusIndex]}
          </p>
        </div>

        <div className="mt-6 h-[3px] w-full max-w-[280px] overflow-hidden rounded-full bg-white/20">
          <div
            className="h-full rounded-full bg-white transition-[width] duration-500 ease-out"
            style={{ width: `${percent}%` }}
          />
        </div>
        <p className="mt-2 text-[12px] tabular-nums text-white/60">
          {Math.round(percent)}%
        </p>

        {error && (
          <div className="mt-6 w-full max-w-sm rounded-xl bg-white/10 px-4 py-3 text-left text-[13px] text-[#ffb4a8]">
            <p>{error}</p>
            <button
              type="button"
              onClick={retry}
              className="mt-3 rounded-full bg-white px-4 py-1.5 text-[13px] font-semibold text-black"
            >
              Retry
            </button>
          </div>
        )}
      </div>

      <div className="absolute bottom-6 right-6 z-10 rounded-full bg-white/85 px-4 py-2 text-[13px] font-medium text-[#333] backdrop-blur-sm">
        Setting up Storbyte
      </div>

      <style>{`
        .noise-overlay {
          background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)' opacity='0.55'/%3E%3C/svg%3E");
          background-size: 180px 180px;
        }
        .status-slide {
          animation: statusSlideUp 0.45s ease-out both;
        }
        @keyframes statusSlideUp {
          from {
            opacity: 0;
            transform: translateY(14px);
          }
          to {
            opacity: 1;
            transform: translateY(0);
          }
        }
      `}</style>
    </div>
  );
}
