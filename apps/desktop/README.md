# StorByte Desktop

Tauri 2 + React + Rust core (WinFsp virtual drive).

## Setup (developers)

1. Install [WinFsp](https://winfsp.dev/)
2. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++** (MSVC `link.exe`). WinFsp bindings do **not** build on the GNU Rust target.
3. Copy `.env.example` → `.env` and set Google OAuth client credentials (used at **build** time — baked into the exe so end users do not need a `.env`). See [docs/google-oauth.md](../../docs/google-oauth.md).
4. `npm install`
5. `npm run tauri dev`

### Release build

```powershell
npx tauri build --no-bundle
```

Binary path: `src-tauri/target/release/storbyte.exe`

End-user installs only need the built exe (plus WinFsp when prompted). No `.env` on the client PC.

## Zero-bit export

Settings → Zero-bit export (default ON). When ON, use Explorer **Copy as Storbyte link…** to create a `.storbytelink` stub on the Desktop instead of downloading file bytes.
