# goo

A small, local-first Markdown editor for **plain `.md` files in folders you choose**, with live preview,
**Mermaid diagrams**, and **31 themes × 18 accent colours**. Built with Tauri 2 (Rust) and Svelte 5.
No account, no telemetry, nothing uploaded.

Plain Markdown, your folders, your colours: read [the Goo Manifesto](#manifesto).

## Features

- **Folders, not databases.** Open any folder: goo lists its Markdown files (`.md`, `.markdown`) as a tree, with a
  quick filter. Notes are saved as plain UTF-8 files with atomic writes (temp file + rename), so a crash never leaves
  half a note behind.
- **Edit, split or preview** (`Ctrl+E`): a CodeMirror 6 editor with Markdown highlighting (and highlighting inside
  fenced code), next to a live GitHub-flavoured preview: tables, task lists, strikethrough, heading
  anchors, front matter shown as a block, syntax-highlighted code.
- **Mermaid diagrams**: a ` ```mermaid ` block becomes a diagram (flowchart, sequence, class, state, ER, Gantt, pie,
  git graph, …), drawn in the current theme and accent colours and redrawn when you switch. Parse errors are shown
  inline with the source.
- **Themes and accents** shared with [Slinger](https://github.com/perunok/slinger): 31 light and dark palettes,
  18 accents layered over any of them, "follow the system", adjustable font size. Every theme × accent pair is
  checked for WCAG AA contrast (AAA for the high-contrast themes) by the test suite.
- **Tabs** with per-tab undo history, unsaved markers and save prompts; the last folder and open notes come back on
  the next start.
- **Links between notes**: `[see this](../other.md#section)` opens the other note and scrolls to the heading. Web
  links open in your browser.
- **Safe by default**: rendered notes are sanitised (no scripts, no event handlers, no remote content), Mermaid runs
  in `strict` mode, and the Rust side refuses any file path outside the folder you opened.

## Keyboard

| Keys | Action |
| --- | --- |
| `Ctrl+O` | Open folder |
| `Ctrl+N` | New note |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / save all |
| `Ctrl+W`, `Ctrl+Tab` | Close tab, next tab |
| `Ctrl+E` | Cycle edit / split / preview |
| `Ctrl+\` | Toggle sidebar |
| `Ctrl+,` | Settings (themes, accents, font) |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Font size |
| `Ctrl+B` / `Ctrl+I` / `Ctrl+K` | Bold / italic / link (in the editor) |

## Getting started

Requirements: Node.js 20+, Rust (stable), and the [Tauri prerequisites](https://tauri.app/start/prerequisites/)
for your OS. On Debian/Ubuntu:

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Then:

```bash
npm install
npm run app:dev       # the desktop app with hot reload
npm run dev           # just the UI in a browser at http://localhost:1420, with an in-memory sample folder
npm run app:build     # installers for the current OS into target/release/bundle/
```

Checks:

```bash
npm run typecheck     # svelte-check
npm test              # Vitest: renderer, themes (contrast of every theme x accent), paths, settings
npm run test:rust     # goo-core: tree listing, path guarding, atomic writes, recents
```

## Releases

Installers are built by GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)) for
Linux (x64 + arm64: AppImage, deb, rpm), macOS (Apple silicon + Intel: dmg) and Windows (x64: setup exe, msi).

```bash
npm version 0.2.0 --no-git-tag-version   # bump package.json (Cargo/Tauri read it from there)
git commit -am "Release 0.2.0"
git tag -a v0.2.0 -m "goo 0.2.0"
git push --follow-tags
```

The tag builds everything and attaches it to a **draft** release; publish it on GitHub when it looks right. Running
the workflow by hand (Actions > Release > Run workflow) builds the same installers as downloadable artifacts without
a release. Builds are not code-signed yet (macOS: right-click > Open the first time; Windows: SmartScreen > Run anyway).

## How it is built

```
src/                 Svelte 5 UI (TypeScript)
  app/               shell, stores (workspace, settings, ui), shortcuts
  backend/           GooBackend interface: tauri.ts (Rust commands) and mock.ts (browser dev mode)
  components/        editor (CodeMirror), markdown (render, highlight, mermaid, view), ui
  features/          files (sidebar, tree, tabs, status bar, welcome), settings, about
  lib/               pure logic: themes, appearance, contrast audit, paths
  styles/themes.css  every colour in the app (31 themes, 18 accents)
src-tauri/           the Tauri shell: window, plugins and the commands the UI calls
crates/goo-core/     file logic in plain Rust (no Tauri), unit-tested on its own
```

- The UI never touches the disk itself; it calls a handful of commands (`list_tree`, `read_file`, `write_file`,
  `create_file`, …). The Rust side keeps the open folder and checks every path against it (symlinks resolved), and
  only reopens folders the user picked before.
- Colours live only in `src/styles/themes.css`; components use `var(--token)`. Mermaid gets the same palette,
  resolved to hex at render time.
- The UI runs in any browser with `npm run dev`; `src/backend/mock.ts` stands in for the Rust side with sample notes.

## Roadmap

- Watch the folder for outside changes (reload / conflict prompt)
- Local images next to notes
- Outline panel, quick open (`Ctrl+P`), command palette
- Rename, move and delete notes; folders
- Export to HTML / PDF
- Custom themes (as in Slinger)

## Manifesto

**The Goo Manifesto**

1. **Plain text outlives every app.** Your notes are .md files in folders you choose. Open them with anything, forever.
2. **Small and quick.** No account, no telemetry, ready in a blink.
3. **Yours, on your disk.** Nothing leaves your machine unless you move it.
4. **Words first.** The editor is the source of truth; the preview just follows.
5. **Diagrams are text too.** Mermaid lives in your notes as code you can diff and review.
6. **Take the shape of the container.** Themes, accents and layouts: goo fits the way you work.
7. **Nothing runs that you didn't write.** Rendered notes can't run scripts or fetch from the web behind your back.
8. **Anyone can pour some goo.** Open source, MIT licensed, built in the open, yours to fork.

*Write plainly. Keep it local. Let it flow.*

The same text is in the app (About goo); it lives in `src/features/about/credits.ts`, so keep the two in step.

## License

[MIT](LICENSE)
