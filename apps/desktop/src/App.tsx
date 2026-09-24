import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { SettingsModal } from "./components/SettingsModal";
import { ShellLayout } from "./components/ShellLayout";
import { TitleBar } from "./components/TitleBar";
import { UploadAccountPicker } from "./components/UploadAccountPicker";
import { useAppState } from "./hooks/useAppState";
import { HomePage } from "./pages/HomePage";
import { IntroPage } from "./pages/IntroPage";
import * as api from "./services/api";

function SettingsRedirect({ onOpen }: { onOpen: () => void }) {
  const navigate = useNavigate();
  useEffect(() => {
    onOpen();
    navigate("/", { replace: true });
  }, [navigate, onOpen]);
  return null;
}

export default function App() {
  const { state, error, busy, syncProgress, run, refresh } = useAppState();
  const [letters, setLetters] = useState<string[]>(["V", "W", "X", "Y", "Z"]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [uploadPick, setUploadPick] = useState<{ name?: string; path?: string } | null>(null);

  const openSettings = useCallback(() => setSettingsOpen(true), []);

  const finishIntro = useCallback(() => {
    void run(async () => {
      await api.updateSettings({
        intro_seen: true,
        onboarded: true,
        launch_at_startup: true,
        auto_mount_on_launch: true,
      });
    });
  }, [run]);

  useEffect(() => {
    void api.availableDriveLetters().then(setLetters).catch(() => undefined);
  }, []);

  // Safety net: auto-mount once if not already mounted (never remount on every state refresh).
  const mountAttempted = useRef(false);
  useEffect(() => {
    if (!state) return;
    if (state.settings.auto_mount_on_launch === false) return;
    if (state.drive.mounted) {
      mountAttempted.current = true;
      return;
    }
    if (mountAttempted.current) return;
    if (state.accounts.filter((a) => a.status === "connected").length === 0) return;
    mountAttempted.current = true;
    const t = window.setTimeout(() => {
      void api
        .mountDrive()
        .then(() => refresh())
        .catch(() => undefined);
    }, 1500);
    return () => window.clearTimeout(t);
  }, [state, refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenCancel: (() => void) | undefined;
    void listen<{ name?: string; path?: string }>("upload-account-pick", (event) => {
      setUploadPick({ name: event.payload?.name, path: event.payload?.path });
    }).then((fn) => {
      unlisten = fn;
    });
    void listen("upload-account-pick-cancelled", () => {
      setUploadPick(null);
    }).then((fn) => {
      unlistenCancel = fn;
    });
    return () => {
      unlisten?.();
      unlistenCancel?.();
    };
  }, []);

  if (!state) {
    return (
      <div className="flex h-full flex-col">
        <TitleBar />
        <div className="flex min-h-0 flex-1 items-center justify-center text-[var(--muted)]">
          {error ?? "Loading Storbyte…"}
        </div>
      </div>
    );
  }

  const needsIntro = !state.settings.intro_seen;

  if (needsIntro) {
    return (
      <div className="flex h-full flex-col">
        <TitleBar />
        <div className="min-h-0 flex-1">
          <IntroPage onContinue={finishIntro} />
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <TitleBar />
      <div className="min-h-0 flex-1">
        <ShellLayout
          onReplayIntro={() => {
            void run(async () => {
              await api.updateSettings({ intro_seen: false });
            });
          }}
          onOpenSettings={openSettings}
          settingsOpen={settingsOpen}
          accountEmail={state.accounts[0]?.email}
          driveCount={state.accounts.filter((a) => a.status === "connected").length}
          accounts={state.accounts}
          drive={state.drive}
          busy={busy}
          onMount={() => run(() => api.mountDrive())}
          onUnmount={() => run(() => api.unmountDrive())}
        >
          {error && (
            <div className="m-4 mb-0 rounded-xl border border-[#e24b4a]/30 bg-[#fdeced] px-3 py-2 text-sm text-[#8a2e2e]">
              {error}
            </div>
          )}
          <Routes>
            <Route
              path="/"
              element={
                <div className="h-full min-h-0">
                  <HomePage
                    drive={state.drive}
                    accounts={state.accounts}
                    busy={busy}
                    syncProgress={syncProgress}
                    onAddAccount={() => run(() => api.connectGoogleAccount())}
                    onMount={() => run(() => api.mountDrive())}
                    onUnmount={() => run(() => api.unmountDrive())}
                    onDisconnect={(id) => run(() => api.disconnectAccount(id))}
                    onReconnect={(id) => run(() => api.reconnectAccount(id))}
                    onRemove={(id) => run(() => api.removeAccount(id))}
                    onRefreshQuotas={() => run(() => api.refreshAccountQuotas())}
                  />
                </div>
              }
            />
            <Route path="/settings" element={<SettingsRedirect onOpen={openSettings} />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </ShellLayout>
      </div>

      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        settings={state.settings}
        accounts={state.accounts}
        letters={letters}
        busy={busy}
        onChange={(patch) => {
          void run(() => api.updateSettings(patch));
        }}
        onAddAccount={() => run(() => api.connectGoogleAccount())}
        onSignOut={(id) => {
          void run(() => api.disconnectAccount(id));
        }}
      />

      <UploadAccountPicker
        open={Boolean(uploadPick)}
        fileName={uploadPick?.name}
        accounts={state.accounts}
        busy={busy}
        onPick={(accountId) => {
          void api.resolveUploadAccount(accountId).then(() => {
            setUploadPick(null);
            void refresh();
          });
        }}
        onCancel={() => {
          void api.cancelUploadAccountPick();
          setUploadPick(null);
        }}
      />
    </div>
  );
}
