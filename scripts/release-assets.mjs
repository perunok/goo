// Collects the installers `tauri build` made and copies them to an output folder under release names in Slinger's
// style: <product>-<version>-<os>-<arch>.<ext>, with each format's own arch spelling (deb: amd64, AppImage/rpm:
// x86_64, macOS/Windows: arm64 / x64), e.g. goo-0.2.0-linux-amd64.deb, goo-0.2.0-mac-arm64.dmg, goo-0.2.0-win-x64.exe.
//
//   node scripts/release-assets.mjs <target> <out-dir>     target: linux-x64 | linux-arm64 | macos-x64 | macos-arm64 | windows-x64
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { basename, join } from 'node:path'
import { fileURLToPath } from 'node:url'

/** bundle folder -> { ext, arch spelling per CPU } */
const FORMATS = {
  deb: { ext: 'deb', arch: { x64: 'amd64', arm64: 'arm64' } },
  rpm: { ext: 'rpm', arch: { x64: 'x86_64', arm64: 'aarch64' } },
  appimage: { ext: 'AppImage', arch: { x64: 'x86_64', arm64: 'aarch64' } },
  dmg: { ext: 'dmg', arch: { x64: 'x64', arm64: 'arm64' } },
  nsis: { ext: 'exe', arch: { x64: 'x64', arm64: 'arm64' } },
  msi: { ext: 'msi', arch: { x64: 'x64', arm64: 'arm64' } },
}
const OS = { linux: 'linux', macos: 'mac', windows: 'win' }

/** The release file name for one installer, or null for files that are not installers (.sig, .app.tar.gz, ...). */
export function releaseName({ product, version, target, format, file }) {
  const [os, cpu] = target.split('-')
  const f = FORMATS[format]
  if (!f || !OS[os] || !f.arch[cpu]) return null
  if (!file.endsWith(`.${f.ext}`)) return null
  // leftovers of an earlier version in target/ must not ship under the new version's name
  if (!file.includes(version)) return null
  return `${product}-${version}-${OS[os]}-${f.arch[cpu]}.${f.ext}`
}

/** Every `release/bundle` folder under target/ (target/release/... and target/<triple>/release/...). */
function bundleDirs(targetDir) {
  const dirs = []
  const direct = join(targetDir, 'release', 'bundle')
  if (existsSync(direct)) dirs.push(direct)
  for (const entry of existsSync(targetDir) ? readdirSync(targetDir) : []) {
    const nested = join(targetDir, entry, 'release', 'bundle')
    if (entry !== 'release' && existsSync(nested)) dirs.push(nested)
  }
  return dirs
}

function main([target, outDir]) {
  if (!target || !outDir) throw new Error('usage: release-assets.mjs <target> <out-dir>')
  const root = fileURLToPath(new URL('..', import.meta.url))
  const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  const conf = JSON.parse(readFileSync(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'))
  const product = conf.productName
  mkdirSync(outDir, { recursive: true })
  const made = []
  for (const dir of bundleDirs(join(root, 'target'))) {
    for (const format of Object.keys(FORMATS)) {
      const formatDir = join(dir, format)
      if (!existsSync(formatDir)) continue
      for (const file of readdirSync(formatDir)) {
        const name = releaseName({ product, version: pkg.version, target, format, file })
        if (!name || !statSync(join(formatDir, file)).isFile()) continue
        if (made.includes(name)) throw new Error(`two installers would be called ${name}`)
        copyFileSync(join(formatDir, file), join(outDir, name))
        made.push(name)
        console.log(`${basename(file)} -> ${name}`)
      }
    }
  }
  if (made.length === 0) throw new Error(`no installers found for ${target}`)
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main(process.argv.slice(2))
