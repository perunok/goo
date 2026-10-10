import { releaseName } from './release-assets.mjs'

const name = (target: string, format: string, file: string) => releaseName({ product: 'goo', version: '0.2.0', target, format, file })

describe('release asset names (Slinger style)', () => {
  it('uses each format’s own arch spelling', () => {
    expect(name('linux-x64', 'deb', 'goo_0.2.0_amd64.deb')).toBe('goo-0.2.0-linux-amd64.deb')
    expect(name('linux-x64', 'appimage', 'goo_0.2.0_amd64.AppImage')).toBe('goo-0.2.0-linux-x86_64.AppImage')
    expect(name('linux-x64', 'rpm', 'goo-0.2.0-1.x86_64.rpm')).toBe('goo-0.2.0-linux-x86_64.rpm')
    expect(name('linux-arm64', 'deb', 'goo_0.2.0_arm64.deb')).toBe('goo-0.2.0-linux-arm64.deb')
    expect(name('linux-arm64', 'appimage', 'goo_0.2.0_aarch64.AppImage')).toBe('goo-0.2.0-linux-aarch64.AppImage')
    expect(name('macos-arm64', 'dmg', 'goo_0.2.0_aarch64.dmg')).toBe('goo-0.2.0-mac-arm64.dmg')
    expect(name('macos-x64', 'dmg', 'goo_0.2.0_x64.dmg')).toBe('goo-0.2.0-mac-x64.dmg')
    expect(name('windows-x64', 'nsis', 'goo_0.2.0_x64-setup.exe')).toBe('goo-0.2.0-win-x64.exe')
    expect(name('windows-x64', 'msi', 'goo_0.2.0_x64_en-US.msi')).toBe('goo-0.2.0-win-x64.msi')
  })

  it('skips files that are not installers', () => {
    expect(name('macos-arm64', 'macos', 'goo.app.tar.gz')).toBeNull()
    expect(name('windows-x64', 'nsis', 'goo_0.2.0_x64-setup.exe.sig')).toBeNull()
    expect(name('linux-x64', 'appimage', 'goo.AppDir')).toBeNull()
    // a stale installer of another version left in target/
    expect(name('linux-x64', 'deb', 'goo_0.1.0_amd64.deb')).toBeNull()
  })
})
