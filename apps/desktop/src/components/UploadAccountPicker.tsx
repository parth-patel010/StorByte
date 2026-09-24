import type { AccountInfo } from "../types";

type Props = {
  open: boolean;
  fileName?: string;
  accounts: AccountInfo[];
  busy?: boolean;
  onPick: (accountId: string) => void;
  onCancel: () => void;
};

function displayName(email: string) {
  const local = email.split("@")[0] || email;
  return local.replace(/[._-]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

export function UploadAccountPicker({
  open,
  fileName,
  accounts,
  busy,
  onPick,
  onCancel,
}: Props) {
  if (!open) return null;
  const connected = accounts.filter((a) => a.status === "connected");

  return (
    <div
      className="accounts-backdrop fixed inset-0 z-[90] flex items-center justify-center bg-black/35 p-6 backdrop-blur-md"
      onClick={onCancel}
      role="presentation"
    >
      <div
        className="accounts-panel w-full max-w-md overflow-hidden rounded-[1.25rem] border border-[#e8e8e8] bg-white shadow-[0_24px_80px_rgba(0,0,0,0.2)]"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Choose upload account"
      >
        <div className="border-b border-[#eee] px-5 py-4">
          <h2 className="text-[16px] font-semibold text-[#1a1a1a]">Upload to which account?</h2>
          <p className="mt-1 text-[13px] text-[#777]">
            {fileName ? (
              <>
                Choose a Drive for <span className="font-medium text-[#333]">{fileName}</span>
              </>
            ) : (
              "Choose a connected Google Drive account"
            )}
          </p>
        </div>
        <div className="sx-scroll max-h-[360px] overflow-y-auto p-2">
          {connected.length === 0 ? (
            <p className="px-3 py-6 text-center text-[13px] text-[#888]">No connected accounts</p>
          ) : (
            connected.map((a) => (
              <button
                key={a.id}
                type="button"
                disabled={busy}
                onClick={() => onPick(a.id)}
                className="flex w-full items-center gap-3 rounded-xl border-0 bg-transparent px-3 py-3 text-left transition hover:bg-[#f5f5f5] disabled:opacity-50"
              >
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-[#1a1a1a] text-[12px] font-semibold text-white">
                  {(a.email[0] ?? "G").toUpperCase()}
                </div>
                <div className="min-w-0">
                  <p className="truncate text-[14px] font-medium text-[#1a1a1a]">
                    {displayName(a.email)}
                  </p>
                  <p className="truncate text-[12px] text-[#888]">{a.email}</p>
                </div>
              </button>
            ))
          )}
        </div>
        <div className="flex justify-end border-t border-[#eee] px-4 py-3">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-lg border border-[#ddd] bg-white px-3 py-1.5 text-[13px] text-[#333] hover:bg-[#f7f7f7]"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
