import { useLayoutEffect, useRef, useState } from "react";
import logoMark from "../assets/stroex-logo-light.png";
import * as api from "../services/api";

type Props = {
  onBack: () => void;
  onContinue: () => void;
  /** Preview path: skip OAuth and open the next intro screen */
  onDemoContinue: (email: string) => void;
};

const leftCol = [
  "/intro/login/a1.jpg",
  "/intro/login/a2.jpg",
  "/intro/login/a3.jpg",
];
const rightCol = [
  "/intro/login/b1.jpg",
  "/intro/login/b2.jpg",
  "/intro/login/b3.jpg",
];

export function IntroStepLogin({ onBack, onContinue, onDemoContinue }: Props) {
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const signInGoogle = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.connectGoogleAccount();
      onContinue();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (!/sign-in cancelled/i.test(msg) && !/sign-in timed out/i.test(msg)) {
        setError(msg);
      }
    } finally {
      setBusy(false);
    }
  };

  const cancelSignIn = async () => {
    await api.cancelGoogleOauth();
  };

  const continueWithEmail = () => {
    const trimmed = email.trim().toLowerCase();
    if (trimmed === "test@gmail.com") {
      onDemoContinue(trimmed);
      return;
    }
    void signInGoogle();
  };

  return (
    <div className="flex h-full overflow-hidden bg-white font-[family-name:var(--font-intro)]">
      {/* Left — auth */}
      <div className="login-pane relative z-[2] flex w-full flex-col px-10 py-8 md:w-[46%] md:px-12 lg:px-16">
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-2 self-start border-0 bg-transparent p-0 text-[15px] font-semibold text-black"
        >
          <img src={logoMark} alt="" className="h-11 w-11 object-contain md:h-12 md:w-12" />
          <span className="text-xl font-semibold tracking-tight md:text-2xl">Storbyte</span>
        </button>

        <div className="flex flex-1 flex-col justify-center py-10">
          <h1 className="text-[clamp(2rem,3.5vw,2.75rem)] font-bold tracking-[-0.03em] text-black">
            Welcome back
          </h1>
          <p className="mt-2 max-w-sm text-[15px] leading-relaxed text-[#8a8a8a]">
            Sign in securely to connect the Storbyte desktop app.
          </p>

          <button
            type="button"
            onClick={() => void (busy ? cancelSignIn() : signInGoogle())}
            className="mt-8 flex h-12 w-full max-w-md items-center justify-center gap-3 rounded-full border border-[#1a1a1a] bg-white text-[15px] font-semibold text-[#1a1a1a] transition hover:bg-[#f7f7f7] active:scale-[0.98]"
          >
            {!busy && (
              <img
                src="/intro/login/google.svg"
                alt=""
                className="h-5 w-5"
                draggable={false}
              />
            )}
            {busy ? "Cancel sign-in" : "Continue with Google"}
          </button>

          <div className="my-6 flex max-w-md items-center gap-3 text-[13px] text-[#b0b0b0]">
            <span className="h-px flex-1 bg-[#e6e6e6]" />
            or
            <span className="h-px flex-1 bg-[#e6e6e6]" />
          </div>

          <label className="block max-w-md">
            <span className="sr-only">Email</span>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="name@company.com"
              className="h-12 w-full rounded-xl border-0 bg-[#f3f3f3] px-4 text-[15px] text-black outline-none ring-0 placeholder:text-[#a0a0a0] focus:bg-[#eeeeee]"
            />
          </label>

          <button
            type="button"
            disabled={busy || !email.trim()}
            onClick={continueWithEmail}
            className="mt-3 flex h-12 w-full max-w-md items-center justify-center rounded-full bg-[#2a2a2a] text-[15px] font-semibold text-white transition hover:bg-[#1a1a1a] active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-40"
          >
            Continue
          </button>

          {error && (
            <p className="mt-4 max-w-md text-left text-sm text-[#c0392b]">
              {error}
            </p>
          )}
        </div>

        <p className="text-[12px] text-[#9a9a9a]">
          By continuing you agree to StorByte&apos;s terms and privacy policy.
        </p>
      </div>

      {/* Right — animated collage */}
      <div className="login-visual relative hidden min-h-0 flex-1 p-5 md:block md:p-6">
        <div className="login-mask relative h-full overflow-hidden rounded-[1.75rem] bg-white">
          <div className="absolute inset-0 flex gap-3 px-3 py-0">
            <MarqueeColumn images={leftCol} direction="up" />
            <MarqueeColumn images={rightCol} direction="down" />
          </div>
        </div>
      </div>

      <style>{`
        .login-pane {
          animation: loginIn 650ms cubic-bezier(0.22, 1, 0.36, 1) both;
        }
        .login-visual {
          animation: loginIn 750ms cubic-bezier(0.22, 1, 0.36, 1) 80ms both;
        }
        @keyframes loginIn {
          from { opacity: 0; transform: translateY(12px); }
          to { opacity: 1; transform: translateY(0); }
        }

        .login-mask {
          -webkit-mask-image: linear-gradient(
            to bottom,
            transparent 0%,
            #000 8%,
            #000 92%,
            transparent 100%
          );
          mask-image: linear-gradient(
            to bottom,
            transparent 0%,
            #000 8%,
            #000 92%,
            transparent 100%
          );
        }

        .login-col-slot {
          flex: 1;
          min-width: 0;
          overflow: hidden;
        }
        /* Animate the content track only — never flex-grow it.
           translate % is relative to the element box; a stretched flex
           child made -50% equal half the panel, not half the strip. */
        .login-col {
          display: flex;
          flex-direction: column;
          width: 100%;
          will-change: transform;
          --marquee-shift: 0px;
        }
        .login-col-set {
          display: flex;
          flex-direction: column;
          gap: 12px;
          padding-bottom: 12px;
        }
        .login-col--up {
          animation: loginScrollUp 6s linear infinite;
        }
        .login-col--down {
          animation: loginScrollDown 6s linear infinite;
        }
        .login-col img {
          width: 100%;
          aspect-ratio: 3 / 4;
          object-fit: cover;
          border-radius: 1.15rem;
          flex-shrink: 0;
          border: 0;
          outline: none;
          background: transparent;
          box-shadow: none;
          display: block;
        }

        @keyframes loginScrollUp {
          from { transform: translate3d(0, 0, 0); }
          to { transform: translate3d(0, calc(-1 * var(--marquee-shift)), 0); }
        }
        @keyframes loginScrollDown {
          from { transform: translate3d(0, calc(-1 * var(--marquee-shift)), 0); }
          to { transform: translate3d(0, 0, 0); }
        }
      `}</style>
    </div>
  );
}

function MarqueeColumn({
  images,
  direction,
}: {
  images: string[];
  direction: "up" | "down";
}) {
  const trackRef = useRef<HTMLDivElement>(null);
  const setRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const track = trackRef.current;
    const set = setRef.current;
    if (!track || !set) return;

    const sync = () => {
      const next = set.nextElementSibling as HTMLElement | null;
      const shift = next
        ? next.offsetTop - set.offsetTop
        : set.offsetHeight;
      if (shift > 0) {
        track.style.setProperty("--marquee-shift", `${shift}px`);
      }
    };

    sync();
    const ro = new ResizeObserver(sync);
    ro.observe(set);
    const imgs = Array.from(set.querySelectorAll("img"));
    imgs.forEach((img) => img.addEventListener("load", sync));
    return () => {
      ro.disconnect();
      imgs.forEach((img) => img.removeEventListener("load", sync));
    };
  }, [images]);

  return (
    <div className="login-col-slot">
      <div
        ref={trackRef}
        className={`login-col ${direction === "up" ? "login-col--up" : "login-col--down"}`}
      >
        <div ref={setRef} className="login-col-set">
          {images.map((src) => (
            <img key={`a-${src}`} src={src} alt="" draggable={false} />
          ))}
        </div>
        <div className="login-col-set" aria-hidden>
          {images.map((src) => (
            <img key={`b-${src}`} src={src} alt="" draggable={false} />
          ))}
        </div>
      </div>
    </div>
  );
}
