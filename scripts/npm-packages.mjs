import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const [version, binDir, outDir] = process.argv.slice(2)
if (!version || !binDir || !outDir) {
  console.error('usage: node scripts/npm-packages.mjs <version> <binaries dir> <out dir>')
  process.exit(2)
}
const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..')
const cargo = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8')
const cargoVersion = cargo.match(/^version = "(.*)"/m)[1]
if (cargoVersion !== version) {
  console.error(`version ${version} does not match Cargo.toml ${cargoVersion}`)
  process.exit(1)
}

const platforms = {
  'darwin-arm64': { os: 'darwin', cpu: 'arm64' },
  'darwin-x64': { os: 'darwin', cpu: 'x64' },
  'linux-arm64': { os: 'linux', cpu: 'arm64', libc: 'glibc' },
  'linux-x64': { os: 'linux', cpu: 'x64', libc: 'glibc' },
  'win32-x64': { os: 'win32', cpu: 'x64' },
}
const main = JSON.parse(fs.readFileSync(path.join(root, 'npm/rust-anything/package.json'), 'utf8'))
const common = {
  version,
  license: main.license,
  repository: main.repository,
  homepage: main.homepage,
}

fs.rmSync(outDir, { recursive: true, force: true })
const order = []
for (const [key, p] of Object.entries(platforms)) {
  const exe = p.os === 'win32' ? 'rust-anything.exe' : 'rust-anything'
  const from = path.join(binDir, `npm-${key}`, exe)
  if (!fs.existsSync(from)) {
    console.error(`missing binary for ${key}: ${from}`)
    process.exit(1)
  }
  const name = `rust-anything-${key}`
  const dir = path.join(outDir, name)
  fs.mkdirSync(path.join(dir, 'bin'), { recursive: true })
  fs.copyFileSync(from, path.join(dir, 'bin', exe))
  fs.chmodSync(path.join(dir, 'bin', exe), 0o755)
  const pkg = {
    name,
    ...common,
    description: `The ${key} binary of rust-anything. Install rust-anything instead.`,
    os: [p.os],
    cpu: [p.cpu],
    ...(p.libc ? { libc: [p.libc] } : {}),
    files: ['bin'],
  }
  fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify(pkg, null, 2) + '\n')
  fs.copyFileSync(path.join(root, 'LICENSE'), path.join(dir, 'LICENSE'))
  fs.writeFileSync(
    path.join(dir, 'README.md'),
    `# ${name}\n\nThe ${key} binary of [rust-anything](https://github.com/erkamyaman/rust-anything). ` +
      'Install `rust-anything`, which picks the right binary for your platform.\n',
  )
  order.push(dir)
}

const mainDir = path.join(outDir, 'rust-anything')
fs.cpSync(path.join(root, 'npm/rust-anything'), mainDir, { recursive: true })
main.version = version
main.optionalDependencies = Object.fromEntries(
  Object.keys(platforms).map((key) => [`rust-anything-${key}`, version]),
)
fs.writeFileSync(path.join(mainDir, 'package.json'), JSON.stringify(main, null, 2) + '\n')
fs.copyFileSync(path.join(root, 'README.md'), path.join(mainDir, 'README.md'))
fs.copyFileSync(path.join(root, 'LICENSE'), path.join(mainDir, 'LICENSE'))
order.push(mainDir)

console.log(order.join('\n'))
