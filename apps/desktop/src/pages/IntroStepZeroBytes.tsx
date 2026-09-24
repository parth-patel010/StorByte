type Props = {
  onBack: () => void;
  onContinue: () => void;
};

export function IntroStepZeroBytes({ onBack, onContinue }: Props) {
  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-[#f8f8f8] font-[family-name:var(--font-intro)]">
      <div className="intro2-copy flex shrink-0 flex-col items-center px-10 pt-10 text-center md:pt-12">
        <h1 className="max-w-[36rem] text-[clamp(1.75rem,3.4vw,2.65rem)] font-bold leading-[1.15] tracking-[-0.03em] text-black">
          Open files by streaming — nothing saved like a download.
        </h1>
        <p className="mt-3 max-w-[32rem] text-[15px] leading-relaxed text-[#8a8a8a] md:text-[16px]">
          Like watching a video in a browser: Storbyte streams ranges from the cloud into
          memory, then drops them when you close the file. No local copy is kept on V:\.
        </p>
      </div>

      <div className="intro2-stage relative z-[1] flex min-h-0 flex-1 items-center justify-center px-10 py-6">
        <img
          src="/intro/properties.png"
          alt="Storbyte streams cloud files without saving a local copy on the virtual drive"
          className="max-h-[min(420px,52vh)] w-auto max-w-[min(100%,560px)] object-contain object-center"
          draggable={false}
        />
      </div>

      <footer className="relative z-[2] flex shrink-0 items-center justify-between px-10 pb-8 pt-2 md:px-12">
        <button
          type="button"
          onClick={onBack}
          className="intro2-back relative z-10 rounded-full border border-[#d8d8d8] bg-white px-7 py-3 text-[15px] font-medium text-[#333] transition duration-150 hover:bg-[#f5f5f5] active:scale-[0.94] active:bg-[#ebebeb]"
        >
          Back
        </button>

        <div className="intro2-progress pointer-events-none absolute inset-x-0 flex items-center justify-center gap-3 text-[13px] text-[#b0b0b0]">
          <span className="inline-flex h-[3px] w-14 overflow-hidden rounded-full bg-[#e5e5e5]">
            <span className="block h-full w-[40%] rounded-full bg-[#1a1a1a]" />
          </span>
          <span>2 of 5</span>
        </div>

        <button
          type="button"
          onClick={onContinue}
          className="intro2-continue relative z-10 rounded-full bg-black px-8 py-3.5 text-[15px] font-semibold text-white transition duration-150 hover:bg-[#222] active:scale-[0.94] active:bg-[#111]"
        >
          Continue
        </button>
      </footer>

      <style>{`
        .intro2-copy {
          animation: intro2In 650ms cubic-bezier(0.22, 1, 0.36, 1) both;
        }
        .intro2-stage {
          animation: intro2In 700ms cubic-bezier(0.22, 1, 0.36, 1) 80ms both;
        }
        .intro2-progress {
          animation: intro2Up 650ms cubic-bezier(0.22, 1, 0.36, 1) 180ms both;
        }
        .intro2-back,
        .intro2-continue {
          animation: intro2Up 650ms cubic-bezier(0.22, 1, 0.36, 1) 180ms both;
        }
        .intro2-back:active,
        .intro2-continue:active {
          animation: none;
          transform: scale(0.94);
        }
        @keyframes intro2In {
          from { opacity: 0; transform: translateY(14px); }
          to { opacity: 1; transform: translateY(0); }
        }
        @keyframes intro2Up {
          from { opacity: 0; transform: translateY(22px); }
          to { opacity: 1; transform: translateY(0); }
        }
      `}</style>
    </div>
  );
}
