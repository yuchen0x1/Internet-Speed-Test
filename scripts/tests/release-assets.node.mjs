import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import test from 'node:test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertCompatibleAsset, missingReleaseAssets } from '../upload-missing-release-assets.mjs';

const repositoryRoot = fileURLToPath(new URL('../..', import.meta.url));

const bytes = (text) => Buffer.from(text);
const hash = (text) => createHash('sha256').update(text).digest('hex');
const manifest = (signature = 'signed-build', extra = {}) => bytes(JSON.stringify({
  version: '1.15.7',
  pub_date: '2026-11-27T00:00:00Z',
  notes: 'release notes',
  platforms: { 'windows-x86_64': { signature, url: 'https://github.com/example/repo/releases/download/v1.15.7/app.exe' } },
  ...extra,
}));

test('empty releases accept all assets', () => {
  assert.deepEqual(missingReleaseAssets(['app.exe', 'app.exe.sig'], [], () => assert.fail(), () => assert.fail()),
    ['app.exe', 'app.exe.sig']);
});

test('complete releases remain untouched without comparing a later rebuild', () => {
  assert.deepEqual(missingReleaseAssets(['app.exe'], ['app.exe'], () => assert.fail(), () => assert.fail()), []);
});

test('matching existing build permits missing signature and metadata', () => {
  assert.deepEqual(missingReleaseAssets(['app.exe', 'app.exe.sig', 'latest.json'], ['app.exe'],
    () => bytes('original installer'), () => bytes('original installer')), ['app.exe.sig', 'latest.json']);
});

test('a rebuilt installer cannot receive a mismatched new signature', () => {
  assert.throws(() => missingReleaseAssets(['app.exe', 'app.exe.sig'], ['app.exe'],
    () => bytes('rebuilt installer'), () => bytes('published installer')), /Refusing to mix/);
});

test('existing signatures must match before adding any missing asset', () => {
  assert.throws(() => assertCompatibleAsset('app.exe.sig', bytes('new signature'), bytes('old signature')), /Refusing to mix/);
  assert.doesNotThrow(() => assertCompatibleAsset('app.exe.sig', bytes('signature\n'), bytes('signature\r\n')));
});

test('updater metadata may change prose and date but must preserve signed download binding', () => {
  assert.doesNotThrow(() => assertCompatibleAsset('latest.json', manifest(), manifest('signed-build', { notes: 'old notes', pub_date: '2026-11-26T00:00:00Z' })));
  assert.throws(() => assertCompatibleAsset('latest.json', manifest(), manifest('different-build')), /Refusing to mix/);
  assert.throws(() => assertCompatibleAsset('latest.json', manifest(), manifest('signed-build', { version: '1.15.6' })), /Refusing to mix/);
  assert.throws(() => assertCompatibleAsset('latest.json', manifest(), bytes('{}')), /Refusing to mix/);
});

test('checksums verify every generated entry and allow extra historical CLI entries', () => {
  const local = bytes(`${hash('installer')}  app.exe\n`);
  assert.doesNotThrow(() => assertCompatibleAsset('SHA256SUMS.txt', local,
    bytes(`${hash('cli')}  cli.zip\r\n${hash('installer')}  app.exe\r\n`)));
  assert.throws(() => assertCompatibleAsset('SHA256SUMS.txt', local,
    bytes(`${hash('different installer')}  app.exe\n`)), /Refusing to mix/);
  assert.throws(() => assertCompatibleAsset('SHA256SUMS.txt', local, bytes('invalid')), /Refusing to mix/);
  assert.throws(() => assertCompatibleAsset('app.exe.sha256', local,
    bytes(`${hash('installer')}  app.exe\n${hash('different installer')}  app.exe\n`)), /Refusing to mix/);
});

test('historical asset repair uses installer-only verification after upload', () => {
  const workflow = fs.readFileSync(path.join(repositoryRoot, '.github/workflows/repair-release-assets.yml'), 'utf8');
  assert.match(workflow, /verify-public-release\.mjs --published "\$RELEASE_TAG" --installer-only/);
});
