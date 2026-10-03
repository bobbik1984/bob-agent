import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const semanticVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/

function read(root, file) {
  return fs.readFileSync(path.join(root, file), 'utf8')
}

function write(root, file, content) {
  fs.writeFileSync(path.join(root, file), content, 'utf8')
}

function replaceExactlyOnce(content, pattern, replacement, file) {
  const globalPattern = new RegExp(pattern.source, pattern.flags.includes('g') ? pattern.flags : `${pattern.flags}g`)
  const matches = [...content.matchAll(globalPattern)]
  if (matches.length !== 1) throw new Error(`${file}: expected one version field, found ${matches.length}`)
  return content.replace(pattern, replacement)
}

function replaceFirst(content, pattern, replacement, file) {
  if (!pattern.test(content)) throw new Error(`${file}: missing version field`)
  return content.replace(pattern, replacement)
}

function replaceJsonVersion(content, version, file, nestedRoot = false) {
  if (!nestedRoot) {
    return replaceFirst(content, /("version"\s*:\s*)"[^"]+"/, `$1"${version}"`, file)
  }
  return replaceExactlyOnce(
    content,
    /("packages"\s*:\s*\{\s*""\s*:\s*\{[^}]*?"version"\s*:\s*)"[^"]+"/s,
    `$1"${version}"`,
    file,
  )
}

function replaceCargoManifestVersion(content, version, file) {
  return replaceExactlyOnce(
    content,
    /^(\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
    `$1"${version}"`,
    file,
  )
}

function replaceCargoLockVersion(content, packageName, version, file) {
  const escapedName = packageName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return replaceExactlyOnce(
    content,
    new RegExp(`^(\\[\\[package\\]\\]\\r?\\nname = "${escapedName}"\\r?\\nversion = )"[^"]+"`, 'm'),
    `$1"${version}"`,
    file,
  )
}

export function versionCode(version) {
  const match = semanticVersion.exec(version)
  if (!match) throw new Error(`Invalid three-part version: ${version}`)
  const [major, minor, patch] = match.slice(1).map(Number)
  if (minor > 999 || patch > 999) throw new Error('Minor and patch versions must be below 1000')
  const code = major * 1_000_000 + minor * 1_000 + patch
  if (!Number.isSafeInteger(code) || code > 2_100_000_000) throw new Error('Android versionCode out of range')
  return code
}

export function expectedFiles(root = projectRoot) {
  const rootPackage = JSON.parse(read(root, 'package.json'))
  const version = rootPackage.version
  const code = versionCode(version)
  const floor = JSON.parse(read(root, 'scripts/version-floor.json')).minimumAndroidVersionCode
  if (!Number.isInteger(floor) || floor < 0) throw new Error('Invalid minimumAndroidVersionCode')

  const files = new Map()
  const derive = (file, transform) => files.set(file, transform(read(root, file), version, file))
  derive('src-tauri/tauri.conf.json', replaceJsonVersion)
  derive('src-tauri/Cargo.toml', replaceCargoManifestVersion)
  derive('src-tauri/Cargo.lock', (content, value, file) => replaceCargoLockVersion(content, 'bob', value, file))
  derive('installer/package.json', replaceJsonVersion)
  derive('installer/src-tauri/tauri.conf.json', replaceJsonVersion)
  derive('installer/src-tauri/Cargo.toml', replaceCargoManifestVersion)
  derive('installer/src-tauri/Cargo.lock', (content, value, file) => replaceCargoLockVersion(content, 'bob-installer', value, file))
  for (const file of ['package-lock.json', 'installer/package-lock.json']) {
    const top = replaceJsonVersion(read(root, file), version, file)
    files.set(file, replaceJsonVersion(top, version, file, true))
  }
  return { version, code, floor, files }
}

export function applyVersionContract(mode, root = projectRoot) {
  if (!['check', 'candidate', 'sync'].includes(mode)) throw new Error('Usage: version-contract.mjs check|candidate|sync')
  const { version, code, floor, files } = expectedFiles(root)
  if (mode === 'candidate' && code <= floor) {
    throw new Error(`Android versionCode ${code} must exceed previous candidate floor ${floor}`)
  }
  const mismatches = [...files].filter(([file, expected]) => read(root, file) !== expected).map(([file]) => file)
  if (mode !== 'sync' && mismatches.length) {
    throw new Error(`Version ${version} differs in: ${mismatches.join(', ')}. Run version-contract.mjs sync and commit the result.`)
  }
  if (mode === 'sync') {
    for (const file of mismatches) write(root, file, files.get(file))
  }
  return { version, code, changed: mismatches }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const result = applyVersionContract(process.argv[2] ?? 'check')
    console.log(`version=${result.version} androidVersionCode=${result.code} ${process.argv[2] === 'sync' ? 'updated' : 'verified'}=${result.changed.length}`)
  } catch (error) {
    console.error(error.message)
    process.exitCode = 1
  }
}
