# Contributing to PromptSack

## Setup

```bash
# Node.js 24+ and Rust 1.88+
npm install
npm test
npm start
```

Linux needs WebKitGTK 4.1 (see README).

## Layout

| Path | Purpose |
|---|---|
| `src-tauri/` | Tauri shell + Rust vault (domain logic) |
| `src/renderer/` | Three-pane UI source |
| `src/shared/` | Types, save-policy, API surface |
| `ui/` | Bundled assets consumed by Tauri |
| `tests/` | Vitest contracts / pure TS |
| `scripts/` | UI build helpers |

## Tests

```bash
npm run test:js
npm run test:rust
npm test
```

## Pull requests

- Keep the domain in Rust (`src-tauri/src/vault/`)
- Prefer small, focused commits with Conventional Commits titles
- Do not commit `node_modules/`, `src-tauri/target/`, model caches, or vault data

## License

Contributions are accepted under **GPL-3.0-only**.
