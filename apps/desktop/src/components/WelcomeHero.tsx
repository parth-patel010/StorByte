type Props = {
  title: string;
};

export function WelcomeHero({ title }: Props) {
  return (
    <section className="flex h-full min-h-[420px] flex-col justify-end pb-8">
      <h1
        className="max-w-xl text-2xl font-normal text-[var(--ink)] md:text-3xl"
        style={{ animation: "fadeUp 700ms ease both" }}
      >
        {title}
      </h1>
      <style>{`
        @keyframes fadeUp {
          from { opacity: 0; transform: translateY(12px); }
          to { opacity: 1; transform: translateY(0); }
        }
      `}</style>
    </section>
  );
}
