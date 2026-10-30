import { fireEvent, render } from '@testing-library/svelte'
import { beforeEach, vi } from 'vitest'
import { backend } from '../../app/backend'
import { settings } from '../../app/settings.svelte'
import { ui } from '../../app/ui.svelte'
import type { MockUpdates } from '../../backend/mock'
import { DEFAULT_APPEARANCE } from '../../lib/appearance'
import { ACCENTS, THEMES } from '../../lib/themes'
import { updates } from '../updates/updates.svelte'
import SettingsDialog from './SettingsDialog.svelte'

const mock = (backend as unknown as { updates: MockUpdates }).updates
const $ = <T extends Element = HTMLElement>(sel: string) => document.querySelector<T>(sel)
const $$ = (sel: string) => [...document.querySelectorAll<HTMLElement>(sel)]
const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? ''
const openTab = async (id: string) => fireEvent.click($(`#settings-${id}`)!)
const stored = () => JSON.parse(localStorage.getItem('goo.ui') ?? '{}')

beforeEach(async () => {
  localStorage.clear()
  settings.setAppearance({ ...DEFAULT_APPEARANCE })
  settings.setUi({ sidebar: true, statusBar: true, restoreSession: true, wrap: true, readingWidth: 820 })
  ui.settingsTab = 'appearance'
  ui.dialog = 'settings'
  ui.updateOpen = false
  updates.resetForTests()
  mock.reset()
  updates.install = await backend.updateInstallState()
})

describe('sections', () => {
  it('lists the five sections with Appearance first, and shows the version', () => {
    render(SettingsDialog)
    expect($$('[role=tab]').map(text)).toEqual(['Appearance', 'Layout & editor', 'goo cloud', 'Updates', 'AI assistants'])
    expect($('#settings-appearance')!.getAttribute('aria-selected')).toBe('true')
    expect(text($('.version'))).toBe(`goo ${__APP_VERSION__}`)
    expect($('#settings-panel-appearance')).not.toBeNull()
  })

  it('switches sections with a click and with the arrow keys', async () => {
    render(SettingsDialog)
    await openTab('layout')
    expect($('#settings-panel-layout')).not.toBeNull()
    expect(ui.settingsTab).toBe('layout')
    await fireEvent.keyDown($('[role=tablist]')!, { key: 'ArrowDown' })
    expect(ui.settingsTab).toBe('cloud')
    await fireEvent.keyDown($('[role=tablist]')!, { key: 'End' })
    expect(ui.settingsTab).toBe('assistants')
    await fireEvent.keyDown($('[role=tablist]')!, { key: 'ArrowDown' }) // wraps around
    expect(ui.settingsTab).toBe('appearance')
  })

  it('opens on the section it was asked for, and Done closes it', async () => {
    ui.settingsTab = 'updates'
    render(SettingsDialog)
    expect($('#settings-panel-updates')).not.toBeNull()
    await fireEvent.click($$('footer button').find((b) => text(b) === 'Done')!)
    expect(ui.dialog).toBeNull()
  })
})

describe('theme gallery', () => {
  it('shows every theme with a live thumbnail drawn in its own palette', () => {
    render(SettingsDialog)
    const cards = $$('[data-theme-option]')
    expect(cards).toHaveLength(THEMES.length)
    const preview = cards.find((c) => c.dataset.themeOption === 'dracula')!.querySelector<HTMLElement>('.preview')!
    expect(preview.dataset.theme).toBe('dracula')
  })

  it('choosing a theme applies it, and it is remembered', async () => {
    render(SettingsDialog)
    await fireEvent.click($('[data-theme-option=nord] input')!)
    expect(settings.appearance.theme).toBe('nord')
    expect(document.documentElement.dataset.theme).toBe('nord')
    expect(JSON.parse(localStorage.getItem('goo.appearance')!).theme).toBe('nord')
    expect($('[data-theme-option=nord]')!.classList.contains('selected')).toBe(true)
  })

  it('filters by name and by light/dark', async () => {
    render(SettingsDialog)
    await fireEvent.input($('input[type=search]')!, { target: { value: 'solarized' } })
    expect($$('[data-theme-option]').map((c) => c.dataset.themeOption)).toEqual(['solarized-light', 'solarized'])
    await fireEvent.input($('input[type=search]')!, { target: { value: 'zzz' } })
    expect($$('[data-theme-option]')).toHaveLength(0)
    expect(text($('.none'))).toBe('No themes match “zzz”.')
    await fireEvent.input($('input[type=search]')!, { target: { value: '' } })
    await fireEvent.click($$('.seg button').find((b) => text(b) === 'Dark')!)
    const shown = $$('[data-theme-option]')
    expect(shown.length).toBe(THEMES.filter((t) => t.scheme === 'dark').length)
    expect(shown.every((c) => THEMES.find((t) => t.id === c.dataset.themeOption)!.scheme === 'dark')).toBe(true)
    expect($('.system')).toBeNull() // "System" is a choice among all themes only
  })

  it('"System" follows the OS with its own light and dark themes', async () => {
    render(SettingsDialog)
    const [light, dark] = $$('.pickers select') as unknown as HTMLSelectElement[]
    await fireEvent.change(light!, { target: { value: 'paper' } })
    await fireEvent.change(dark!, { target: { value: 'tokyo-night' } })
    expect(settings.appearance).toMatchObject({ theme: 'system', systemLight: 'paper', systemDark: 'tokyo-night' })
    // only light themes can be the light one
    expect([...light!.options].every((o) => THEMES.find((t) => t.id === o.value)!.scheme === 'light')).toBe(true)
    await fireEvent.click($('[data-theme-option=github-dark] input')!)
    await fireEvent.click($('.system input')!)
    expect(settings.appearance.theme).toBe('system')
  })
})

describe('accent and font size', () => {
  it('offers the theme default and every accent', () => {
    render(SettingsDialog)
    expect($$('input[name=accent]')).toHaveLength(ACCENTS.length + 1)
  })

  it('an accent is applied to the whole app and "Theme default" removes it again', async () => {
    render(SettingsDialog)
    await fireEvent.click($('input[name=accent][value=teal]')!)
    expect(document.documentElement.dataset.accent).toBe('teal')
    expect(settings.appearance.accent).toBe('teal')
    // every thumbnail previews the chosen accent
    expect($('[data-theme-option=light] .preview')!.dataset.accent).toBe('teal')
    await fireEvent.click($('input[name=accent][value=theme]')!)
    expect(document.documentElement.dataset.accent).toBeUndefined()
  })

  it('the font size slider scales the text and shows its value', async () => {
    render(SettingsDialog)
    await fireEvent.input($('#font-size')!, { target: { value: '18' } })
    expect(settings.appearance.fontSize).toBe(18)
    expect(document.documentElement.style.getPropertyValue('--font-size')).toBe('18px')
    expect(text($('label[for=font-size]'))).toBe('Font size: 18px')
  })
})

describe('layout & editor', () => {
  const checkbox = (label: string) => $$('label.check').find((l) => text(l).startsWith(label))!.querySelector<HTMLInputElement>('input')!

  it('toggles the sidebar, status bar, wrapping and restoring notes, and remembers them', async () => {
    ui.settingsTab = 'layout'
    render(SettingsDialog)
    for (const [label, key] of [
      ['Show the sidebar', 'sidebar'],
      ['Show the status bar', 'statusBar'],
      ['Wrap long lines', 'wrap'],
      ['Reopen the last folder', 'restoreSession'],
    ] as const) {
      expect(checkbox(label).checked, label).toBe(true)
      await fireEvent.click(checkbox(label))
      expect(stored()[key], label).toBe(false)
      expect(settings.ui[key]).toBe(false)
    }
  })

  it('the reading width slider and the full-width switch work together', async () => {
    ui.settingsTab = 'layout'
    render(SettingsDialog)
    const slider = $<HTMLInputElement>('#reading-width')!
    await fireEvent.input(slider, { target: { value: '1000' } })
    expect(settings.ui.readingWidth).toBe(1000)
    await fireEvent.click(checkbox('Use the full window width'))
    expect(settings.ui.readingWidth).toBe(0)
    expect(slider.disabled).toBe(true)
    expect($$('.row .faint').some((e) => text(e) === 'Full')).toBe(true)
    await fireEvent.click(checkbox('Use the full window width'))
    expect(settings.ui.readingWidth).toBe(820)
  })
})

describe('updates section', () => {
  const newer = `0.${Number(__APP_VERSION__.split('.')[1]) + 1}.0`

  it('before any check there is only Check now, and the toggle reflects the preference', async () => {
    ui.settingsTab = 'updates'
    render(SettingsDialog)
    const toggle = $$('label.check input')[0] as HTMLInputElement
    expect(toggle.checked).toBe(true)
    await fireEvent.click(toggle)
    expect(updates.auto).toBe(false)
    expect(JSON.parse(localStorage.getItem('goo.updates')!).auto).toBe(false)
    expect(text($('[data-testid=update-status]'))).toBe('Check now')
  })

  it('says "up to date" with when it checked', async () => {
    mock.latest = __APP_VERSION__
    ui.settingsTab = 'updates'
    render(SettingsDialog)
    await fireEvent.click($$('[data-testid=update-status] button').find((b) => text(b) === 'Check now')!)
    await vi.waitFor(() => expect(text($('[data-testid=update-status]'))).toContain('Up to date. Checked just now.'))
    expect($('.dot')).toBeNull() // nothing to attend to
  })

  it('offers Update now for a newer version, marks the section, and opens the update dialog', async () => {
    ui.settingsTab = 'updates'
    render(SettingsDialog)
    await fireEvent.click($$('[data-testid=update-status] button').find((b) => text(b) === 'Check now')!)
    await vi.waitFor(() => expect(text($('[data-testid=update-status]'))).toContain(`goo ${newer} is available.`))
    expect($('#settings-updates .dot')).not.toBeNull()
    await fireEvent.click($$('[data-testid=update-status] button').find((b) => text(b) === 'Update now')!)
    expect(ui.updateOpen).toBe(true)
    await vi.waitFor(() => expect(updates.install?.phase).toBe('downloading'))
  })

  it('an installation that cannot update itself gets View release and the reason', async () => {
    mock.method = null
    updates.install = await backend.updateInstallState()
    const open = vi.spyOn(backend, 'openExternal').mockResolvedValue()
    ui.settingsTab = 'updates'
    render(SettingsDialog)
    await fireEvent.click($$('[data-testid=update-status] button').find((b) => text(b) === 'Check now')!)
    await vi.waitFor(() => expect($$('[data-testid=update-status] button').some((b) => text(b) === 'View release')).toBe(true))
    expect(text($('.small'))).toContain('development build')
    await fireEvent.click($$('[data-testid=update-status] button').find((b) => text(b) === 'View release')!)
    expect(open).toHaveBeenCalledWith(`https://github.com/perunok/goo/releases/tag/v${newer}`)
    open.mockRestore()
  })

  it('shows a failed check without hiding the button to retry', async () => {
    mock.failCheck = 'Could not reach GitHub to check for updates'
    ui.settingsTab = 'updates'
    render(SettingsDialog)
    await fireEvent.click($$('[data-testid=update-status] button').find((b) => text(b) === 'Check now')!)
    await vi.waitFor(() => expect(text($('.bad'))).toContain('Could not check: Could not reach GitHub'))
    expect($$('[data-testid=update-status] button').some((b) => text(b) === 'Check now')).toBe(true)
  })
})
