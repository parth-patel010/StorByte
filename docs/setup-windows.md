# Windows setup (StorByte)

## WinFsp

1. Download and install WinFsp from https://winfsp.dev/
2. Reboot if the installer requests it.
3. Confirm your chosen drive letter (default `V:`) is free before mounting.

StorByte loads `winfsp-x64.dll` from the WinFsp install directory (typically `C:\Program Files (x86)\WinFsp\bin\`). Mount no longer hard-exits the app if loading fails — you’ll get an on-screen error instead.

## Visual Studio Build Tools (required to compile)

Install [VS 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload so `link.exe` is available.

**Use the MSVC Rust toolchain only** (`rustup default stable-x86_64-pc-windows-msvc`). The `winfsp` crate does not support `x86_64-pc-windows-gnu`.

## Google OAuth

1. Open [Google Cloud Console](https://console.cloud.google.com/).
2. Create a project → APIs & Services → enable **Google Drive API**.
3. Create OAuth client ID type **Desktop app**.
4. Add authorized redirect URI: `http://127.0.0.1:17832/oauth/callback` (StorByte loopback).
5. Copy client ID and secret into `apps/desktop/.env`:

```env
GOOGLE_CLIENT_ID=your-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-secret
```

Full steps: [google-oauth.md](google-oauth.md)

Prototype scopes: `https://www.googleapis.com/auth/drive` (read + write FS ops). Prefer tightening later.

## Toolchains

- Node.js 20+
- Rust stable (`rustup`, prefer `x86_64-pc-windows-msvc`)
- Go 1.22+ (optional API)
- Docker Desktop (optional, for Postgres)

## Run the desktop app

```powershell
cd apps\desktop
copy .env.example .env
npm install
npm run tauri dev
```
