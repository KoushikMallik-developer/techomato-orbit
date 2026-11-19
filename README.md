# Techomato Orbit

A free Windows desktop app that puts your local dev machine in one window: the services you run,
the ports in use, installed software, and machine health. It lives in the system tray and works
offline.

Built with Tauri 2 (Rust) + React + TypeScript. Design: `docs/superpowers/specs/`.

Inspired by the macOS app Nexus Console. No code or assets are shared with it.

## Develop

Needs Node 22.12+, Rust (rustup, MSVC toolchain), and the Visual Studio C++ Build Tools.

```bash
npm ci
npm start          # tauri dev
npm test           # vitest
cargo test --manifest-path src-tauri/Cargo.toml
```
