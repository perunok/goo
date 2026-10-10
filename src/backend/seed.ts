/** Sample notes for the in-memory backend (browser dev mode and tests). */
export const SEED_ROOT = '/home/you/notes'

export const SEED_FILES: Record<string, string> = {
  'Welcome.md': `# Welcome to goo

goo is a small Markdown editor for **plain \`.md\` files** in folders you choose.
Nothing is uploaded, there is no account, and your notes stay readable by any other editor.

## Try it

- Switch between **Edit**, **Split** and **Preview** with \`Ctrl+E\`
- Save with \`Ctrl+S\`, start a note with \`Ctrl+N\`
- Pick a theme and an accent colour in **Settings** (\`Ctrl+,\`)
- Write a \`mermaid\` code block and it turns into a diagram (see [Diagrams](Diagrams.md))

## Formatting

| Syntax | Result |
| --- | --- |
| \`**bold**\` | **bold** |
| \`_italic_\` | _italic_ |
| \`~~strike~~\` | ~~strike~~ |
| \`\` \`code\` \`\` | \`code\` |

- [x] Plain files
- [x] Themes and accents
- [ ] Your next idea

> Write plainly. Keep it local. Let it flow.

\`\`\`ts
export function greet(name: string): string {
  return \`Hello, \${name}!\` // highlighted with the theme's syntax colours
}
\`\`\`
`,
  'Diagrams.md': `# Diagrams

Mermaid diagrams follow the active theme and accent.

## Flowchart

\`\`\`mermaid
flowchart LR
  A[Write Markdown] --> B{Has a mermaid block?}
  B -- yes --> C[Render diagram]
  B -- no --> D[Render text]
  C --> E[Preview]
  D --> E
\`\`\`

## Sequence

\`\`\`mermaid
sequenceDiagram
  participant You
  participant goo
  participant Disk
  You->>goo: Ctrl+S
  goo->>Disk: write temp file
  goo->>Disk: rename over note
  Disk-->>goo: ok
  goo-->>You: Saved
\`\`\`

## Pie

\`\`\`mermaid
pie title Where the time goes
  "Writing" : 55
  "Thinking" : 30
  "Choosing a theme" : 15
\`\`\`

## Gantt

\`\`\`mermaid
gantt
  title goo roadmap
  dateFormat YYYY-MM-DD
  section Core
  Editor and preview   :done,    a1, 2026-10-10, 5d
  Mermaid              :done,    a2, after a1, 3d
  section Next
  File watching        :active,  b1, after a2, 5d
  Local images         :         b2, after b1, 4d
\`\`\`

## A broken one

\`\`\`mermaid
flowchart LR
  A -->
\`\`\`
`,
  'Tables.md': `# Tables

| Feature | Status | Owner | Notes |
| --- | :---: | --- | --- |
| Open with | done | goo | Files from any file manager open in goo, in their own folder or the one already open |
| Reading width | done | goo | Narrow, medium, wide or full; also a slider |
| File watching | planned | — | Reload notes that change on disk, ask when there are unsaved edits |
| Export | idea | — | HTML and PDF |

| Shortcut | Action | Shortcut | Action | Shortcut | Action |
| --- | --- | --- | --- | --- | --- |
| \`Ctrl+S\` | Save | \`Ctrl+N\` | New note | \`Ctrl+E\` | Edit / split / preview |
| \`Ctrl+O\` | Open folder | \`Ctrl+W\` | Close tab | \`Ctrl+,\` | Settings |

| Endpoint | Description |
| --- | --- |
| \`https://api.example.com/v1/projects/{projectId}/environments/{environmentId}/variables\` | A very long URL that cannot wrap nicely |
`,
  'journal/2026-10-10.md': `# 10 October 2026

Started **goo** today. Same idea as Slinger: small, fast, yours.
`,
  'projects/goo/ideas.md': `# Ideas

1. Outline panel from the headings
2. Wiki links: \`[[note name]]\`
3. Export to HTML and PDF
`,
}
