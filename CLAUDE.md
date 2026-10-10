# CLAUDE.md

goo: a local-first Markdown editor. Tauri 2 (Rust) shell + Svelte 5 / TypeScript UI. Sibling of Slinger
(`../fpostman/slinger`, Electron): same theme system and philosophy (README "Manifesto").

## Commands

```bash
npm run dev            # UI only, browser, in-memory mock backend (src/backend/mock.ts) on :1420
npm run app:dev        # real desktop app (needs the Tauri Linux prerequisites, see README)
npm run typecheck      # svelte-check
npm test               # vitest (jsdom): src/**/*.test.ts
npm run test:rust      # cargo test -p goo-core
npm run app:build      # installers
```

`.claude/launch.json` has `goo-web` (the browser dev mode) for previews. Vite 8: use its default minifier
(esbuild is not installed).

## Layout and rules

- `crates/goo-core`: all file logic (tree, path guard, atomic write, recents), no Tauri dependency, unit-tested.
  `src-tauri/src/lib.rs`: thin commands over it. Every path from the UI goes through `resolve_inside` against the open
  root; `open_workspace` only accepts folders already in the recent list (picked via the native dialog).
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
