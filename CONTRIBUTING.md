# Contributing to Sidearm

Thanks for considering contributing! Here's how to get started.

## Development Setup

```bash
# Prerequisites
# - Node.js 22+
# - Rust 1.85+ (edition 2024; CI pins 1.94.0) — rustup.rs
# - Windows 10/11

git clone https://github.com/danscMax/Sidearm.git
cd Sidearm
npm install
cargo tauri dev
```

## Project Structure

```
src/                  # React frontend (TypeScript)
  components/         # UI components
  hooks/              # React hooks
  lib/                # Utilities, types, helpers
src-tauri/            # Rust backend
  src/
    lib.rs            # IPC commands, app setup
    capture_backend/  # Keyboard/mouse hooks, side-button reader, foreground watcher
    executor.rs       # Action execution (shortcuts, macros, text)
    resolver.rs       # Profile/button resolution
    config.rs         # Config schema + validation
    input_synthesis.rs # SendInput wrapper + clipboard operations (STA thread)
    device_catalog.rs # Supported mouse models
    mouse_memory.rs   # Vendor HID protocol: onboard memory, DPI, poll rate
    onboard_guard.rs  # Keeps the mouse's onboard memory in Sidearm's hands
```

Each folder's `AGENTS.md` describes its contracts — read them before changing that area.

## Making Changes

1. Create a branch: `git checkout -b fix/description`
2. Make your changes
3. Run checks (CI runs the same):
   ```bash
   npm run check                       # tsc + knip + clippy
   npm run test                        # vitest
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path src-tauri/Cargo.toml
   ```
   Tests marked `#[ignore]` need a connected mouse and are not run by CI — see
   [Adding a mouse model](docs/ADDING-A-MODEL.md).
4. Commit with a clear message: `fix: description` or `feat: description`
5. Open a pull request

## Code Style

- **Rust**: no clippy overrides
- **TypeScript**: project tsconfig (strict, ES2022)
- **React**: functional components, hooks only, no Context API
- **Comments**: English in code; every UI string goes through i18next (`en.json` + `ru.json`)
- **Commits**: [Conventional Commits](https://www.conventionalcommits.org/) format

## Reporting Bugs

Use the [Bug Report](https://github.com/danscMax/Sidearm/issues/new?template=bug_report.yml) issue template.

## Supporting a New Mouse

Owners of a recognised but unverified model can send a
[Device Report](https://github.com/danscMax/Sidearm/issues/new?template=device_report.yml)
with the **Help verify** button in the Mouse tab. How a model gets added and
verified: [docs/ADDING-A-MODEL.md](docs/ADDING-A-MODEL.md).

## Suggesting Features

Use the [Feature Request](https://github.com/danscMax/Sidearm/issues/new?template=feature_request.yml) issue template.
