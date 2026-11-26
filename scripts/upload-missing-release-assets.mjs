import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const digest = (data) => createHash('sha256').update(data).digest('hex');

function checksums(data) {
  const entries = new Map();
  for (const line of data.toString('utf8').trim().split(/\r?\n/)) {
    const match = line.match(/^([\da-f]{64}) [ *](.+)$/i);
    if (!match || entries.has(match[2])) throw new Error('Invalid or duplicate release checksum');
    entries.set(match[2], match[1].toLowerCase());
  }
  return entries;
}

function updaterBinding(data) {
  const manifest = JSON.parse(data.toString('utf8'));
  if (typeof manifest.version !== 'string' || !manifest.platforms?.['windows-x86_64']?.signature) {
    throw new Error('Invalid existing updater manifest');
  }
  return { version: manifest.version, platforms: manifest.platforms };
}

export function assertCompatibleAsset(name, local, existing) {
  try {
    if (name === 'latest.json') {
      // Publication time and release prose can change without changing the signed download.
      assert.deepEqual(updaterBinding(local), updaterBinding(existing));
    } else if (name === 'SHA256SUMS.txt' || name.endsWith('.sha256')) {
      const published = checksums(existing);
      for (const [filename, hash] of checksums(local)) assert.equal(published.get(filename), hash);
    } else if (name.endsWith('.sig')) {
      assert.equal(local.toString('utf8').trim(), existing.toString('utf8').trim());
    } else {
      assert.equal(digest(local), digest(existing));
    }
  } catch {
    throw new Error(`Release asset differs from this build: ${name}. Refusing to mix published assets with a new build.`);
  }
}

export function missingReleaseAssets(localNames, existingNames, readLocal, readExisting) {
  const existing = new Set(existingNames);
  const missing = localNames.filter((name) => !existing.has(name));
  if (!missing.length) return [];
  for (const name of localNames) {
    if (existing.has(name)) assertCompatibleAsset(name, readLocal(name), readExisting(name));
  }
  return missing;
}

export function uploadMissingReleaseAssets(tag, directory = 'release-artifacts') {
  if (!/^v\d+\.\d+\.\d+$/.test(tag || '')) throw new Error('A stable release tag is required');
  const localNames = fs.readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile()).map((entry) => entry.name).sort();
  if (!localNames.length || localNames.some((name) => !/^[\w][\w.-]*$/.test(name))) {
    throw new Error('Missing release assets or unsupported asset filename');
  }
  const gh = (args) => execFileSync('gh', args, { encoding: 'utf8', maxBuffer: 10_000_000 });
  const release = JSON.parse(gh(['release', 'view', tag, '--json', 'assets']));
  if (!Array.isArray(release.assets)) throw new Error('GitHub returned no release asset list');
  const existingNames = release.assets.map((asset) => asset.name);
  const overlap = localNames.filter((name) => existingNames.includes(name));
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'netvision-release-check-'));
  try {
    if (localNames.some((name) => !existingNames.includes(name)) && overlap.length) {
      gh(['release', 'download', tag, '--dir', temporary, ...overlap.flatMap((name) => ['--pattern', name])]);
    }
    const missing = missingReleaseAssets(localNames, existingNames,
      (name) => fs.readFileSync(path.join(directory, name)),
      (name) => fs.readFileSync(path.join(temporary, name)));
    if (missing.length) {
      gh(['release', 'upload', tag, ...missing.map((name) => path.join(directory, name))]);
      console.log(`Uploaded ${missing.length} verified missing release assets.`);
    } else {
      console.log(`Release ${tag} already contains every generated asset.`);
    }
    return missing;
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  uploadMissingReleaseAssets(process.argv[2], process.argv[3]);
}
