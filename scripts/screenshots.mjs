// README screenshots (docs/screenshots/*.png) of the real UI in browser mode, with the sample notes of the in-memory
// backend. Start the UI first (`npm run dev`), then: `node scripts/screenshots.mjs [http://localhost:1420]`.
// Needs Playwright's Chromium: `npx playwright install chromium`.
import { chromium } from 'playwright'
import { mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const BASE = process.argv[2] ?? 'http://localhost:1420'
const OUT = fileURLToPath(new URL('../docs/screenshots/', import.meta.url))
const ROOT = '/home/you/notes'
mkdirSync(OUT, { recursive: true })

const browser = await chromium.launch()
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** A fresh page with a theme, accent, layout and open notes (the app restores them like a real session). */
async function page({ theme, accent = 'theme', viewMode = 'split', open = ['Diagrams.md'], width = 820 }) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })
  await context.addInitScript(
    ([appearance, ui, session]) => {
      localStorage.setItem('goo.appearance', appearance)
      localStorage.setItem('goo.ui', ui)
      localStorage.setItem('goo.lastSession', session)
    },
    [
      JSON.stringify({ v: 1, theme, accent, systemLight: 'light', systemDark: 'dark', fontSize: 14 }),
      JSON.stringify({ viewMode, sidebar: true, wrap: true, readingWidth: width }),
      JSON.stringify({ root: ROOT, open: open.map((f) => `${ROOT}/${f}`), active: `${ROOT}/${open[0]}` }),
    ],
  )
  const p = await context.newPage()
  await p.goto(BASE)
  await p.waitForSelector('.sidebar .row')
  await sleep(1500) // Mermaid's chunk and the first diagrams
  return p
}

async function shot(p, name) {
  await p.screenshot({ path: `${OUT}${name}.png` })
  console.log(`docs/screenshots/${name}.png`)
}

async function settingsTab(p, label) {
  await p.click('button[aria-label^="Settings"]')
  await p.getByRole('tab', { name: label }).click()
  await sleep(300)
}

// 1. split view with Mermaid diagrams
let p = await page({ theme: 'tokyo-night', accent: 'violet', open: ['Diagrams.md', 'Welcome.md', 'Tables.md'] })
await shot(p, '01-split-mermaid')

// 2. preview in a light theme
p = await page({ theme: 'github-light', viewMode: 'preview', open: ['Welcome.md', 'Diagrams.md'] })
await shot(p, '02-preview-light')

// 3. tables + reading width control
p = await page({ theme: 'catppuccin-mocha', accent: 'teal', viewMode: 'preview', open: ['Tables.md'], width: 1100 })
await p.click('button[aria-label="Reading width"]')
await sleep(200)
await shot(p, '03-tables-reading-width')

// 4. themes and accents
p = await page({ theme: 'rose-pine-dawn', accent: 'teal' })
await settingsTab(p, 'Appearance')
await shot(p, '04-themes')

// 5. goo cloud (pretend server of the in-memory backend)
p = await page({ theme: 'nord', accent: 'sky' })
await settingsTab(p, 'goo cloud')
await p.fill('input[aria-label="Server address"]', 'https://goo.example.com')
await p.getByRole('button', { name: 'Sign in' }).click()
await p.waitForSelector('text=Signed in as', { timeout: 10_000 })
await p.getByRole('button', { name: 'Sync this folder…' }).click()
await p.selectOption('.linkform select', 'ws-team')
await p.getByRole('button', { name: 'Start syncing' }).click()
await sleep(300)
await shot(p, '05-goo-cloud')

// 6. AI assistants (MCP)
p = await page({ theme: 'dracula' })
await settingsTab(p, 'AI assistants')
await p.getByLabel('Allow AI assistants to use goo').check()
await sleep(300)
await shot(p, '06-ai-assistants')

// 7. updates (the in-memory backend pretends a newer release exists)
p = await page({ theme: 'github-dark', accent: 'blue' })
await settingsTab(p, 'Updates')
await p.getByRole('button', { name: 'Check now' }).click()
await p.waitForSelector('text=is available', { timeout: 10_000 })
await sleep(300)
await shot(p, '08-updates')

// 8. welcome screen
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
await context.addInitScript(() => localStorage.setItem('goo.appearance', JSON.stringify({ v: 1, theme: 'midnight', accent: 'violet', systemLight: 'light', systemDark: 'dark', fontSize: 14 })))
p = await context.newPage()
await p.goto(BASE)
await sleep(800)
await shot(p, '07-welcome')

await browser.close()
