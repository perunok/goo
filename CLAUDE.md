# CLAUDE.md

goo: a local-first Markdown editor. Tauri 2 (Rust) shell + Svelte 5 / TypeScript UI. Sibling of Slinger
(`../fpostman/slinger`, Electron): same theme system and philosophy (README "Manifesto").

## Commands

```bash
npm run dev            # UI only, browser, in-memory mock backend (src/backend/mock.ts) on :1420
npm run app:dev        # real desktop app (needs the Tauri Linux prerequisites, see README)
npm run typecheck      # svelte-check
npm test               # vitest (jsdom): src/**/*.test.ts
npm run test:rust      # cargo test -p goo-core -p goo-mcp -p goo-sync
npm run app:build      # installers
```

`.claude/launch.json` has `goo-web` (the browser dev mode) for previews. Vite 8: use its default minifier
(esbuild is not installed).

## Layout and rules

- `crates/goo-core`: all file logic (tree, path guard, atomic write, recents), no Tauri dependency, unit-tested.
  `src-tauri/src/lib.rs`: thin commands over it. Every path from the UI goes through `resolve_inside` against the open
  root; `open_workspace` only accepts folders already in the recent list (picked via the native dialog).
- `crates/goo-mcp`: MCP server (no Tauri). `tools.rs` (note tools + `Host` trait), `protocol.rs` (stateless JSON-RPC),
  `http.rs` (tiny_http on 127.0.0.1, Bearer token, Host check, no Origin), `bridge.rs` (`goo --mcp-stdio`, handled in
  `src-tauri/src/main.rs` before Tauri starts; starts the app when `launch.json` allows), `files.rs` (settings /
  endpoint.json / launch.json in `<config>/mcp/`, 0600). `src-tauri/src/mcp.rs` implements `Host` and the settings
  commands; the UI reports unsaved notes (`set_unsaved`) and reloads on `files-changed`. Same design as Slinger's
  `electron/mcp/`. The bridge must not pool HTTP connections (a stopped endpoint would hang it).
- `crates/goo-sync`: goo cloud sync engine (no Tauri). `api.rs` (wire types, `Remote`
  trait), `http.rs` (`HttpRemote`, ureq + rustls; `TokenStore` = keychain, single-flight refresh, persist before use;
  `DeviceSignIn`), `state.rs` (`<state_dir>/state.json` + `base/<note_id>`, never in the notes folder), `scan.rs`
  (changes derived from files vs. base, rename = same hash), `merge.rs` (diffy 3-way), `engine.rs` (`sync()`: pull,
  push, up to 4 rounds; conflict copies, `may_write` defers notes with unsaved edits). Tests use an in-process fake of
  the server (`src/tests/fake.rs`); `tests/real_server.rs` runs against goo-cloud when `GOO_SYNC_IT_SERVER_DIR` is set.
- goo cloud (desktop side): `crates/goo-sync` is the engine (spec: goo-cloud `docs/DESIGN.md` §4); `src-tauri/src/cloud.rs`
  holds the account (`<config>/cloud/account.json`, tokens in the OS keychain via `keyring` 3), folder links
  (`links.json`), per-workspace state dirs, and one scheduler thread (60 s, sooner after a save / Sync now, backoff).
  It emits `cloud-status` and `files-changed`. UI: `src/features/cloud/` (store, Settings tab, status bar). Real-server
  test: `GOO_SYNC_IT_SERVER_DIR=../goo-cloud/server cargo test -p goo-sync --test real_server`.
- Folder watching: `src-tauri/src/watch.rs` (notify-debouncer-mini, follows the open root in `activate`) emits
  `files-changed` with `tree: true`; `goo_core::classify_change` filters like the tree. The store reloads clean notes,
  marks dirty ones `diskChanged` (save then asks keep mine / load theirs) and ignores its own save echoes (same text).
- `src-tauri/src/uibridge.rs`: Rust asks the window (`ui-query` event -> `ui_answer` command), used by the MCP tools
  `get_open_note` (text incl. unsaved edits + selection from `components/editor/selection.ts`) and `check_mermaid`
  (`mermaidError` in `mermaid.ts`). Answers live in `src/app/uiQueries.ts`.
- `npm run screenshots` (with `npm run dev` running) regenerates `docs/screenshots/` via Playwright.
- UI talks to the outside only through `GooBackend` (`src/backend/types.ts`). Adding a command touches:
  goo-core (+ test) -> `src-tauri/src/lib.rs` (+ `generate_handler!`) -> `types.ts` -> `tauri.ts` -> `mock.ts`.
  Window-level JS APIs need a permission in `src-tauri/capabilities/default.json`.
- Colours only in `src/styles/themes.css` (ported from Slinger; the `var-*` and HTTP-method tokens were dropped).
  Components use `var(--token)`; `src/styles/themes.test.ts` fails on literal colours anywhere in `src/` and checks
  every theme x accent for contrast. Register new themes in `src/lib/themes.ts`.
- Markdown: `components/markdown/render.ts` (marked + DOMPurify allowlist, class filter `md-*`/`tok-*`),
  `highlight.ts` (Lezer -> `tok-*`), `mermaid.ts` (lazy chunk, `securityLevel: 'strict'`, theme variables resolved
  from tokens via a canvas; renders are queued). `MarkdownView.svelte` draws diagrams after `tick()`.
- Stores are `*.svelte.ts` classes (`app/workspace`, `app/settings`, `app/ui`). Errors become toasts.
- Svelte 5 scoped styles add only one class of specificity; global element styles in `app.css` are wrapped in
  `:where()` so components win.
- CSP is in `src-tauri/tauri.conf.json`; `style-src 'unsafe-inline'` is needed for Mermaid's SVG `<style>` and
  CodeMirror, hence `dangerousDisableAssetCspModification: ["style-src"]`.
- `public/theme-init.js` mirrors `lib/appearance.ts` (key `goo.appearance`) to theme the first paint.
