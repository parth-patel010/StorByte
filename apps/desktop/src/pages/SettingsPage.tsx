import type { AppSettings } from "../types";

type Props = {
  settings: AppSettings;
  letters: string[];
  busy: boolean;
  onChange: (patch: Partial<AppSettings>) => void;
};

const cacheOptions = [
  { label: "Disabled (0)", value: 0 },
  { label: "256 MB", value: 256 * 1024 * 1024 },
  { label: "512 MB", value: 512 * 1024 * 1024 },
  { label: "1 GB", value: 1024 * 1024 * 1024 },
  { label: "5 GB", value: 5 * 1024 * 1024 * 1024 },
  { label: "10 GB", value: 10 * 1024 * 1024 * 1024 },
];

export function SettingsPage({ settings, letters, busy, onChange }: Props) {
  return (
    <div className="mx-auto flex max-w-xl flex-col gap-8 px-8 py-8 text-[#111]">
      <section>
        <h2 className="text-2xl font-semibold tracking-tight">Virtual Drive</h2>
        <p className="mt-1 text-sm text-[#8a8a8e]">Choose which letter Storbyte mounts.</p>
        <select
          className="mt-3 w-full rounded-xl border border-[#e4e4e7] bg-white px-3 py-2.5 text-[#111]"
          value={settings.drive_letter}
          disabled={busy}
          onChange={(e) => onChange({ drive_letter: e.target.value })}
        >
          {[settings.drive_letter, ...letters.filter((l) => l !== settings.drive_letter)].map(
            (letter) => (
              <option key={letter} value={letter}>
                {letter}:
              </option>
            ),
          )}
        </select>
      </section>

      <section>
        <h2 className="text-2xl font-semibold tracking-tight">Zero-bit export</h2>
        <p className="mt-1 text-sm text-[#8a8a8e]">
          When on, drag/copy from the virtual drive to your PC creates a tiny .storbytelink stub
          instead of downloading the file. Opening files on V: still streams (range reads) —
          Explorer Size on disk may look ≈ Size due to WinFsp, but content is not stored as a
          local download on V:. Turn off to copy full content off the drive (uses disk space).
        </p>
        <label className="mt-4 flex cursor-pointer items-center gap-3">
          <input
            type="checkbox"
            checked={settings.zero_bit_export}
            disabled={busy}
            onChange={(e) => onChange({ zero_bit_export: e.target.checked })}
            className="size-4 accent-[#111]"
          />
          <span>Zero-bit on (link stubs)</span>
        </label>
      </section>

      <section>
        <h2 className="text-2xl font-semibold tracking-tight">Cache</h2>
        <p className="mt-1 text-sm text-[#8a8a8e]">
          Speeds up reopening large files (ZIPs, etc.). Even when Disabled, Storbyte keeps a
          small RAM working set so Explorer does not re-download the same blocks.
        </p>
        <select
          className="mt-3 w-full rounded-xl border border-[#e4e4e7] bg-white px-3 py-2.5 text-[#111]"
          value={settings.cache_bytes}
          disabled={busy}
          onChange={(e) => onChange({ cache_bytes: Number(e.target.value) })}
        >
          {cacheOptions.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </section>

      <section>
        <h2 className="text-2xl font-semibold tracking-tight">About</h2>
        <p className="mt-2 text-sm text-[#8a8a8e]">
          Storbyte Phase 1 prototype — virtual cloud drive with on-demand streaming.
        </p>
      </section>
    </div>
  );
}
