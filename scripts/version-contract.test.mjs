import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'
import { applyVersionContract, versionCode } from './version-contract.mjs'

const sourceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const files = [
  'package.json',
  'package-lock.json',
  'scripts/version-floor.json',
  'src-tauri/tauri.conf.json',
  'src-tauri/Cargo.toml',
  'src-tauri/Cargo.lock',
  'installer/package.json',
  'installer/package-lock.json',
  'installer/src-tauri/tauri.conf.json',
  'installer/src-tauri/Cargo.toml',
  'installer/src-tauri/Cargo.lock',
]

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'bob-version-contract-'))
  t.after(() => {
    if (!root.startsWith(path.join(os.tmpdir(), 'bob-version-contract-'))) {
      throw new Error('Refusing to clean up an unexpected test directory')
    }
    fs.rmSync(root, { recursive: true, force: true })
  })
  for (const file of files) {
    const target = path.join(root, file)
    fs.mkdirSync(path.dirname(target), { recursive: true })
    fs.copyFileSync(path.join(sourceRoot, file), target)
  }
  return root
}

test('current checked-in manifests agree on one product version', () => {
  assert.deepEqual(applyVersionContract('check').changed, [])
  assert.equal(versionCode('0.9.24'), 9024)
})

test('installer version drift fails instead of silently building a mismatched bundle', (t) => {
  const root = fixture(t)
  const file = path.join(root, 'installer/package.json')
  fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('"version": "0.9.24"', '"version": "0.9.7"'), 'utf8')
  assert.throws(() => applyVersionContract('check', root), /installer\/package\.json/)
  assert.ok(applyVersionContract('sync', root).changed.includes('installer/package.json'))
  assert.deepEqual(applyVersionContract('check', root).changed, [])
})

test('candidate gate requires a newer Android versionCode than reserved recovery', (t) => {
  const root = fixture(t)
  const floorFile = path.join(root, 'scripts/version-floor.json')
  fs.writeFileSync(floorFile, JSON.stringify({ minimumAndroidVersionCode: 9024 }), 'utf8')
  assert.throws(() => applyVersionContract('candidate', root), /must exceed previous candidate floor 9024/)
  const file = path.join(root, 'package.json')
  fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('"version": "0.9.24"', '"version": "0.9.25"'), 'utf8')
  applyVersionContract('sync', root)
  assert.equal(applyVersionContract('candidate', root).code, 9025)
})

test('invalid product version is rejected before any file is changed', (t) => {
  const root = fixture(t)
  const file = path.join(root, 'package.json')
  fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('"version": "0.9.24"', '"version": "0.9"'), 'utf8')
  assert.throws(() => applyVersionContract('sync', root), /Invalid three-part version/)
  assert.throws(() => versionCode('0.9.1000'), /below 1000/)
})

test('both candidate workflows verify versions and cannot publish a release', () => {
  for (const workflow of ['windows.yml', 'android.yml']) {
    const content = fs.readFileSync(path.join(sourceRoot, '.github/workflows', workflow), 'utf8')
    assert.match(content, /- mobile-pc-rebuild/)
    assert.match(content, /node scripts\/version-contract\.mjs candidate/)
    assert.match(content, /actions\/upload-artifact@v4/)
    assert.doesNotMatch(content, /action-gh-release|latest-desktop|latest-mobile|bob-mobile-latest\.apk/)
  }
})
