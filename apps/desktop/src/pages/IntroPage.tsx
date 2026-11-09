import { useState, type CSSProperties, type MouseEvent } from "react";
import { createPortal } from "react-dom";
import logoMark from "../assets/stroex-logo-light.png";
import { IntroStepZeroBytes } from "./IntroStepZeroBytes";
import { IntroStepDependencies } from "./IntroStepDependencies";
import { IntroStepLogin } from "./IntroStepLogin";
import { IntroStepConnect } from "./IntroStepConnect";
import { IntroStepFetching } from "./IntroStepFetching";

type Props = {
  onContinue: () => void;
};

const photos = [
  "/intro/1.jpg",
  "/intro/2.jpg",
  "/intro/3.jpg",
  "/intro/4.jpg",
  "/intro/5.jpg",
  "/intro/2.jpg",
];

const diskLabels = ["1.8 GB", "420 MB", "2.4 GB", "860 MB", "3.1 GB", "4.6 GB"];

type Slot = {
  x: string;
  y: string;
  rotate: string;
  enterRotate: string;
  dx: string;
  dy: string;
  z: number;
  delay: number;
  size: string;
};

function crescentSlots(count: number) {
  const cx = 126;
  const cy = 50;
  const radius = 78;
  const startDeg = 142;
  const endDeg = 218;

  return Array.from({ length: count }, (_, i) => {
    const t = count === 1 ? 0.5 : i / (count - 1);
    const deg = startDeg + (endDeg - startDeg) * t;
    const rad = (deg * Math.PI) / 180;

    let rotate = (deg - 180) * 0.26;
    if (i === 0 || i === count - 1) rotate *= 0.55;

    let x = cx + radius * Math.cos(rad);
    const y = cy - radius * Math.sin(rad);
    if (i === 0 || i === count - 1) x += 6;

    const enterDeg = deg - 32;
    const enterRad = (enterDeg * Math.PI) / 180;
    const enterR = radius + 22;
    const fromX = cx + enterR * Math.cos(enterRad);
    const fromY = cy - enterR * Math.sin(enterRad);

    return {
      x: `${x}%`,
      y: `${y}%`,
      rotate: `${rotate}deg`,
      enterRotate: `${rotate - 28}deg`,
      dx: `${fromX - x}%`,
      dy: `${fromY - y}%`,
      z: 10 + i,
      delay: 80 + i * 80,
      size: diskLabels[i % diskLabels.length],
    };
  });
}

function IntroPhotoCard({
  src,
  slot,
  index,
}: {
  src: string;
  slot: Slot;
  index: number;
}) {
  const [hover, setHover] = useState(false);
  const [tip, setTip] = useState({ x: 0, y: 0 });

  const onMove = (e: MouseEvent<HTMLDivElement>) => {
    // Viewport coords — tip is portaled outside transformed ancestors
    setTip({
      x: e.clientX + 16,
      y: e.clientY - 44,
    });
  };

  return (
    <div
      className={`intro-card${hover ? " is-hover" : ""}`}
      style={
        {
          ["--r" as string]: slot.rotate,
          ["--r0" as string]: slot.enterRotate,
          ["--dx" as string]: slot.dx,
          ["--dy" as string]: slot.dy,
          animationDelay: `${slot.delay}ms`,
        } as CSSProperties
      }
      onMouseEnter={(e) => {
        setHover(true);
        setTip({ x: e.clientX + 16, y: e.clientY - 44 });
      }}
      onMouseLeave={() => setHover(false)}
      onMouseMove={onMove}
    >
      <div className="intro-card-media">
        <img
          src={src}
          alt=""
          className="intro-card-img"
          style={{ objectPosition: `${16 + ((index * 14) % 42)}% center` }}
          draggable={false}
        />
      </div>

      {hover &&
        createPortal(
          <div
            className="intro-float is-on"
            style={{ left: tip.x, top: tip.y }}
          >
            <div className="intro-hover-pill">
              <span className="font-semibold text-white">{slot.size}</span>
              <span className="text-[#c8c8c8]">Zero bytes on disk</span>
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}

export function IntroPage({ onContinue }: Props) {
  const [step, setStep] = useState(1);
  const [demoEmail, setDemoEmail] = useState("test@gmail.com");
  const slots = crescentSlots(photos.length);

  if (step === 6) {
    return <IntroStepFetching onDone={onContinue} />;
  }

  if (step === 5) {
    return (
      <IntroStepConnect
        email={demoEmail}
        onBack={() => setStep(4)}
        onContinue={() => setStep(6)}
      />
    );
  }

  if (step === 4) {
    return (
      <IntroStepLogin
        onBack={() => setStep(3)}
        onContinue={() => setStep(5)}
        onDemoContinue={(email) => {
          setDemoEmail(email);
          setStep(5);
        }}
      />
    );
  }

  if (step === 3) {
    return (
      <IntroStepDependencies
        onBack={() => setStep(2)}
        onContinue={() => setStep(4)}
      />
    );
  }

  if (step === 2) {
    return (
      <IntroStepZeroBytes
        onBack={() => setStep(1)}
        onContinue={() => setStep(3)}
      />
    );
  }

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-[#f8f8f8]">
      <div className="intro-copy pointer-events-none relative z-[2] flex min-h-0 flex-1 items-center pl-12 md:pl-16">
        <div className="pointer-events-auto max-w-[22rem]">
          <img
            src={logoMark}
            alt="Storbyte"
            className="h-14 w-14 object-contain md:h-16 md:w-16"
          />
          <h1 className="mt-7 font-[family-name:var(--font-intro)] text-[clamp(2.5rem,5.2vw,4.15rem)] leading-[1.06] tracking-[-0.04em]">
            <span className="block font-bold text-black">No more</span>
            <span className="block whitespace-nowrap font-normal text-[#8e8e8e]">
              Hard drive and SSD
            </span>
          </h1>
        </div>
      </div>

      <div
        className="absolute top-[48%] right-0 z-[4] overflow-visible"
        style={{
          width: "min(720px, 70vh)",
          height: "min(720px, 70vh)",
          right: "-6%",
          transform: "translateY(-50%)",
        }}
      >
        {photos.map((src, i) => {
          const slot = slots[i];
          return (
            <div
              key={`${src}-${i}`}
              className="absolute"
              style={{
                left: slot.x,
                top: slot.y,
                zIndex: slot.z,
                width: 0,
                height: 0,
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.zIndex = "50";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.zIndex = String(slot.z);
              }}
            >
              <IntroPhotoCard src={src} slot={slot} index={i} />
            </div>
          );
        })}
      </div>

      <footer className="pointer-events-none relative z-[5] flex shrink-0 items-center justify-end px-12 pb-8 pt-1 md:px-16">
        <div className="intro-progress absolute inset-x-0 flex items-center justify-center gap-3 font-[family-name:var(--font-intro)] text-[13px] text-[#b0b0b0]">
          <span className="inline-flex h-[3px] w-12 overflow-hidden rounded-full bg-[#e5e5e5]">
            <span className="block h-full w-[22%] rounded-full bg-[#1a1a1a]" />
          </span>
          <span>1 of 5</span>
        </div>
        <button
          type="button"
          onClick={() => setStep(2)}
          className="intro-continue pointer-events-auto relative z-[1] rounded-full bg-black px-8 py-3.5 font-[family-name:var(--font-intro)] text-[15px] font-semibold text-white transition duration-150 hover:bg-[#222] active:scale-[0.94] active:bg-[#111]"
        >
          Continue
        </button>
      </footer>

      <style>{`
        .intro-copy {
          animation: copySlideRight 750ms cubic-bezier(0.22, 1, 0.36, 1) both;
        }
        .intro-progress {
          animation: progressSlideUp 700ms cubic-bezier(0.22, 1, 0.36, 1) 320ms both;
        }
        .intro-continue {
          animation: progressSlideUp 700ms cubic-bezier(0.22, 1, 0.36, 1) 380ms both;
        }
        .intro-continue:active {
          animation: none;
          transform: scale(0.94);
        }
        @keyframes copySlideRight {
          from { opacity: 0; transform: translateX(-36px); }
          to { opacity: 1; transform: translateX(0); }
        }
        @keyframes progressSlideUp {
          from { opacity: 0; transform: translateY(28px); }
          to { opacity: 1; transform: translateY(0); }
        }

        .intro-card {
          position: absolute;
          left: 0;
          top: 0;
          width: min(278px, 34vmin);
          aspect-ratio: 2 / 1;
          border-radius: 1.1rem;
          overflow: visible;
          background: transparent;
          box-shadow: 0 10px 28px rgba(0, 0, 0, 0.14);
          transform: translate(-50%, -50%) rotate(var(--r));
          transform-origin: center center;
          animation: cardOrbitIn 900ms cubic-bezier(0.16, 1, 0.3, 1) both;
          cursor: default;
          pointer-events: auto;
        }
        .intro-card-media {
          position: absolute;
          inset: 0;
          overflow: hidden;
          border-radius: 1.1rem;
          background: #ececec;
        }
        .intro-card.is-hover {
          box-shadow: none;
        }
        .intro-card.is-hover .intro-card-media {
          box-shadow: 0 16px 36px rgba(0, 0, 0, 0.22);
        }
        .intro-card-img {
          display: block;
          height: 100%;
          width: 100%;
          object-fit: cover;
          transform: scale(1);
          transition: transform 320ms cubic-bezier(0.22, 1, 0.36, 1);
          will-change: transform;
        }
        .intro-card.is-hover .intro-card-img {
          transform: scale(1.08);
        }

        /* Floating pill — fixed to viewport, free of image bounds */
        .intro-float {
          position: fixed;
          z-index: 9999;
          pointer-events: none;
          opacity: 1;
        }
        .intro-hover-pill {
          display: flex;
          align-items: center;
          gap: 12px;
          white-space: nowrap;
          padding: 9px 16px;
          border-radius: 999px;
          background: rgba(28, 28, 28, 0.92);
          font-family: var(--font-intro), system-ui, sans-serif;
          font-size: 12.5px;
          line-height: 1.2;
          box-shadow: 0 6px 18px rgba(0, 0, 0, 0.28);
        }

        @keyframes cardOrbitIn {
          0% {
            opacity: 0;
            transform:
              translate(-50%, -50%)
              translate(var(--dx), var(--dy))
              rotate(var(--r0))
              scale(0.86);
          }
          60% { opacity: 1; }
          100% {
            opacity: 1;
            transform: translate(-50%, -50%) rotate(var(--r)) scale(1);
          }
        }
      `}</style>
    </div>
  );
}
