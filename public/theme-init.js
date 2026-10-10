// Applies the persisted theme and accent before first paint (external file: the CSP forbids inline scripts).
// Mirrors src/lib/appearance.ts (key `goo.appearance`); settings.init() validates and re-applies right after startup.
try {
  var d = document.documentElement
  var a = {}
  try {
    a = JSON.parse(localStorage.getItem('goo.appearance') || '{}') || {}
  } catch (e) {}
  var t = a.theme || 'system'
  if (t === 'system') {
    t = matchMedia('(prefers-color-scheme: light)').matches ? a.systemLight || 'light' : a.systemDark || 'dark'
  }
  d.setAttribute('data-theme', t)
  if (a.accent && a.accent !== 'theme') d.setAttribute('data-accent', a.accent)
  if (typeof a.fontSize === 'number' && a.fontSize >= 11 && a.fontSize <= 22) d.style.setProperty('--font-size', a.fontSize + 'px')
} catch (e) {}
