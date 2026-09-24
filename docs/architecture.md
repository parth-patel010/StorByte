# Stroex architecture (Phase 1)

```text
Tauri UI (React)
      │ IPC
Rust core
  ├── config / settings
  ├── auth (Google OAuth PKCE + Credential Manager)
  ├── metadata (SQLite)
  ├── filesystem (WinFsp)
  ├── blocks / cache / scheduler / prefetch
  ├── export (.stroexlink)
  └── providers::GoogleDriveProvider
          │
    Google Drive API (direct from PC)
```

Control plane (`services/api`) handles health, future auth/license/device only.
It must never proxy file bytes.
