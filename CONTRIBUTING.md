# Contributing to StorByte

Thanks for helping make StorByte better. This project is free and open source — small, focused PRs are welcome.

## Ways to contribute

- Fix bugs / improve reliability (mount, OAuth, delete/rename, streaming)
- Improve docs (setup, OAuth screenshots, troubleshooting)
- UX polish in the desktop UI
- Tests for Rust metadata / block cache / prefetch logic
- Ideas via GitHub Issues (search first to avoid duplicates)

## Development setup

Follow the root [README](README.md) and [docs/setup-windows.md](docs/setup-windows.md).

Minimum path:

```powershell
cd apps\desktop
copy .env.example .env   # add your Google OAuth Desktop client
npm install
npm run tauri dev
```

You need WinFsp + MSVC Rust (`stable-x86_64-pc-windows-msvc`).

## Pull request guidelines

1. Fork the repo and create a branch: `fix/short-description` or `feat/short-description`
2. Keep changes focused — one concern per PR when possible
3. Do not commit `.env`, secrets, `node_modules`, or local `target/` / `dist/` binaries
4. Describe **what** and **why** in the PR body; include repro steps for bugfixes
5. If you touch streaming/cache behavior, note any expected performance tradeoffs

## Code of conduct

Be respectful. Assume good intent. Harassment or spam will not be tolerated.

## License

By contributing, you agree your contributions are licensed under the MIT License.
