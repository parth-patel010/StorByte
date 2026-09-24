# StorByte

**Turn multiple Google Drive accounts into one fast virtual Windows drive.**

StorByte mounts a drive letter (e.g. `V:`) and streams files on demand — no full-cloud sync, no filling your disk. Open large videos and archives like local files; metadata lives in SQLite; bytes flow **PC ↔ Google Drive** only.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D4)](docs/setup-windows.md)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

> Free and open source. Contributions welcome.

## Why StorByte?

| Problem | StorByte |
| --- | --- |
| Google Drive sync eats disk | Streams on demand (WinFsp) |
| Multiple accounts = multiple folders | One merged virtual drive |
| Large files feel slow to open | First-byte + adaptive prefetch |
| Sharing a file means downloading it | Optional **zero-bit** `.storbytelink` stubs |

## Features

- Connect one or more Google accounts (OAuth)
- Mount a WinFsp virtual drive (`V:` by default)
- Browse from a local SQLite index
- HTTP Range reads for large files (no full download first)
- Fast local rename/delete; Drive updates in the background
- Auto-mount / launch at Windows startup
- Zero-bit export (tiny link stubs instead of copying bytes)

## Repository layout

```text
apps/desktop          Tauri 2 + React + TypeScript + Rust (main app)
services/api          Optional Go control plane (auth scaffold — not a file proxy)
docs/                 Setup, OAuth, architecture
```

## Prerequisites (Windows)

| Tool | Notes |
| --- | --- |
| [WinFsp](https://winfsp.dev/) | Required to mount the virtual drive |
| [VS 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) | **Desktop development with C++** (MSVC `link.exe`) |
| [Rust](https://rustup.rs/) | `rustup default stable-x86_64-pc-windows-msvc` (GNU target will not build WinFsp bindings) |
| [Node.js](https://nodejs.org/) 20+ | Frontend + Tauri CLI |
| Google Cloud OAuth Desktop client | See [docs/google-oauth.md](docs/google-oauth.md) |
| Go 1.22+ + Docker | Optional — only for `services/api` |

Full walkthrough: **[docs/setup-windows.md](docs/setup-windows.md)**

## Quick start (developers)

```powershell
# 1. Clone
git clone https://github.com/parth-patel010/StorByte.git
cd StorByte

# 2. Google OAuth credentials (build-time; baked into the exe)
cd apps\desktop
copy .env.example .env
# Edit .env → set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET
# Details: docs/google-oauth.md

# 3. Install + run
npm install
npm run tauri dev
```

### Build a release `.exe`

```powershell
cd apps\desktop
npm install
npx tauri build --no-bundle
# Binary: apps\desktop\src-tauri\target\release\storbyte.exe
```

End users only need the built exe + WinFsp. They do **not** need a `.env` if you baked OAuth at build time.

### Optional API control plane

```powershell
docker compose up -d
cd services\api
go run ./cmd\server
```

Health: `GET http://localhost:8080/health`

## Security & privacy

- StorByte never asks for your Google password (browser OAuth)
- Refresh tokens live in **Windows Credential Manager**
- File bytes travel **User PC ↔ Google Drive** only — the optional API must not proxy normal file data
- Do **not** commit `.env` (gitignored). Rotate any secret that was ever committed

## Contributing

See **[CONTRIBUTING.md](CONTRIBUTING.md)**. Bug reports, docs fixes, and small PRs are the fastest way to help.

## License

[MIT](LICENSE) — free to use, modify, and distribute.
