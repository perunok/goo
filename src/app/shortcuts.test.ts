import { settings } from './settings.svelte'
import { handleZoomWheel } from './shortcuts'
import { ui } from './ui.svelte'

const page = document.body.appendChild(document.createElement('div'))
page.className = 'content'
const frame = document.body.appendChild(document.createElement('div'))
for (const el of [page, frame]) el.addEventListener('wheel', handleZoomWheel)

const wheel = (init: WheelEventInit, over: Element = page) => {
  const e = new WheelEvent('wheel', { cancelable: true, bubbles: true, ...init })
  over.dispatchEvent(e)
  return e
}

describe('Ctrl+scroll zoom', () => {
  beforeEach(() => settings.setAppearance({ fontSize: 14 }))

  it('leaves plain scrolling alone', () => {
    const e = wheel({ deltaY: -120 })
    expect(e.defaultPrevented).toBe(false)
    expect(settings.appearance.fontSize).toBe(14)
  })

  it('moves one step per mouse notch, whatever its size', () => {
    expect(wheel({ ctrlKey: true, deltaY: -240 }).defaultPrevented).toBe(true)
    expect(settings.appearance.fontSize).toBe(15)
    wheel({ ctrlKey: true, deltaY: 3, deltaMode: 1 })
    expect(settings.appearance.fontSize).toBe(14)
  })

  it('adds up small trackpad deltas, and a change of direction starts over', () => {
    for (let i = 0; i < 3; i++) wheel({ ctrlKey: true, deltaY: -10 })
    expect(settings.appearance.fontSize).toBe(14)
    wheel({ ctrlKey: true, deltaY: 10 })
    wheel({ ctrlKey: true, deltaY: -30 })
    expect(settings.appearance.fontSize).toBe(14)
    wheel({ ctrlKey: true, deltaY: -10 })
    expect(settings.appearance.fontSize).toBe(15)
  })

  it('stops at the limits', () => {
    for (let i = 0; i < 40; i++) wheel({ ctrlKey: true, deltaY: -100 })
    expect(settings.appearance.fontSize).toBe(22)
    for (let i = 0; i < 40; i++) wheel({ ctrlKey: true, deltaY: 100 })
    expect(settings.appearance.fontSize).toBe(11)
  })

  it('does nothing over the sidebar or tabs, but still blocks the page zoom', () => {
    expect(wheel({ ctrlKey: true, deltaY: -100 }, frame).defaultPrevented).toBe(true)
    expect(settings.appearance.fontSize).toBe(14)
  })

  it('does nothing behind a dialog, but still blocks the page zoom', () => {
    ui.open('settings')
    expect(wheel({ ctrlKey: true, deltaY: -100 }).defaultPrevented).toBe(true)
    expect(settings.appearance.fontSize).toBe(14)
    ui.close()
  })
})
