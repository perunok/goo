/** Which dialog is open, plus toasts. One place, so any part of the app can open any dialog. */
export type DialogName = 'settings' | 'about' | null

export type SettingsTab = 'appearance' | 'layout' | 'cloud' | 'updates' | 'assistants'
export const SETTINGS_TABS: { id: SettingsTab; label: string }[] = [
  { id: 'appearance', label: 'Appearance' },
  { id: 'layout', label: 'Layout & editor' },
  { id: 'cloud', label: 'goo cloud' },
  { id: 'updates', label: 'Updates' },
  { id: 'assistants', label: 'AI assistants' },
]

export interface Toast {
  id: number
  kind: 'error' | 'info'
  text: string
  /** A smaller second line. */
  detail?: string
  /** A button (e.g. "Update now"); the toast closes when it is pressed. */
  action?: { label: string; run: () => void }
}

export interface ConfirmRequest {
  title: string
  message: string
  /** Buttons, the first one is the default. */
  actions: { id: string; label: string; kind?: 'primary' | 'danger' }[]
  resolve: (id: string | null) => void
}

export interface PromptRequest {
  title: string
  label: string
  value: string
  confirmLabel: string
  resolve: (value: string | null) => void
}

class Ui {
  dialog = $state<DialogName>(null)
  /** The Update goo dialog (it can sit on top of Settings). */
  updateOpen = $state(false)
  settingsTab = $state<SettingsTab>('appearance')
  toasts = $state<Toast[]>([])
  confirmRequest = $state<ConfirmRequest | null>(null)
  /** The MCP tool an AI assistant is running (shown in the status bar for a moment). */
  assistantTool = $state<string | null>(null)
  private assistantTimer: ReturnType<typeof setTimeout> | null = null
  promptRequest = $state<PromptRequest | null>(null)
  private nextId = 1

  open(d: Exclude<DialogName, null>, tab?: SettingsTab) {
    if (tab) this.settingsTab = tab
    this.dialog = d
  }

  close() {
    this.dialog = null
  }

  assistantBusy(tool: string) {
    this.assistantTool = tool
    if (this.assistantTimer) clearTimeout(this.assistantTimer)
    this.assistantTimer = setTimeout(() => (this.assistantTool = null), 2500)
  }

  /** `ms` 0 keeps the toast until it is dismissed. */
  toast(text: string, kind: Toast['kind'] = 'info', ms = kind === 'error' ? 6000 : 2500, extra: Pick<Toast, 'detail' | 'action'> = {}) {
    const id = this.nextId++
    this.toasts = [...this.toasts, { id, kind, text, ...extra }]
    if (ms > 0) setTimeout(() => this.dismiss(id), ms)
    return id
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id)
  }

  /** Resolves with the chosen action id, or null when dismissed. */
  confirm(req: Omit<ConfirmRequest, 'resolve'>): Promise<string | null> {
    return new Promise((resolve) => {
      this.confirmRequest = {
        ...req,
        resolve: (id) => {
          this.confirmRequest = null
          resolve(id)
        },
      }
    })
  }

  prompt(req: Omit<PromptRequest, 'resolve'>): Promise<string | null> {
    return new Promise((resolve) => {
      this.promptRequest = {
        ...req,
        resolve: (v) => {
          this.promptRequest = null
          resolve(v)
        },
      }
    })
  }
}

export const ui = new Ui()
