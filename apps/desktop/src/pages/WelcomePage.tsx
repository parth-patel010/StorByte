import { WelcomeHero } from "../components/WelcomeHero";

type Props = {
  busy: boolean;
  onDone: () => Promise<unknown>;
};

/** Kept for compatibility; welcome CTA was removed — App goes straight to Space. */
export function WelcomePage(_props: Props) {
  return (
    <WelcomeHero title="Turn your cloud storage into a drive on your PC." />
  );
}

export async function connectFlow() {
  const api = await import("../services/api");
  await api.connectGoogleAccount();
}
