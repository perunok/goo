/** Which dialog is open, plus toasts. One place, so any part of the app can open any dialog. */
export type DialogName = 'settings' | 'about' | null

export interface Toast {
  id: number
  kind: 'error' | 'info'
  text: string
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
  toasts = $state<Toast[]>([])
  confirmRequest = $state<ConfirmRequest | null>(null)
  promptRequest = $state<PromptRequest | null>(null)
  private nextId = 1

  open(d: Exclude<DialogName, null>) {
    this.dialog = d
  }

  close() {
    this.dialog = null
  }

  toast(text: string, kind: Toast['kind'] = 'info', ms = kind === 'error' ? 6000 : 2500) {
    const id = this.nextId++
    this.toasts = [...this.toasts, { id, kind, text }]
    setTimeout(() => this.dismiss(id), ms)
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
