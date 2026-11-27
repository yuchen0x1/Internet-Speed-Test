import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash, createPublicKey, verify } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { pathToFileURL } from 'node:url';

function decodeBase64(text) {
  assert.equal(typeof text, 'string', 'Base64 value must be text');
  const value = text.trim();
  assert.match(value, /^[A-Za-z0-9+/]+={0,2}$/);
  const result = Buffer.from(value, 'base64');
  assert.equal(result.toString('base64').replace(/=+$/, ''), value.replace(/=+$/, ''));
  return result;
}

function releaseContext(config, expectedVersion) {
  const version = String(expectedVersion || config.version).replace(/^v/, '');
  assert.match(version, /^\d+\.\d+\.\d+$/, 'A stable release version is required');
  const updater = config.plugins?.updater;
  const publicKeyText = decodeBase64(updater?.pubkey).toString('utf8').trim().split(/\r?\n/);
  assert.equal(publicKeyText.length, 2);
  assert.ok(publicKeyText[0].startsWith('untrusted comment: '));
  const publicKeyBytes = decodeBase64(publicKeyText[1]);
  assert.equal(publicKeyBytes.length, 42);
  assert.ok(['Ed', 'ED'].includes(publicKeyBytes.subarray(0, 2).toString('ascii')));
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), publicKeyBytes.subarray(10)]),
    type: 'spki',
    format: 'der',
  });
  const endpoint = new URL(updater.endpoints[0]);
  assert.equal(endpoint.protocol, 'https:');
  assert.equal(endpoint.hostname, 'github.com');
  assert.equal(endpoint.username + endpoint.password + endpoint.search + endpoint.hash, '');
  const repository = endpoint.pathname.split('/').filter(Boolean).slice(0, 2).join('/');
  assert.match(repository, /^[\w.-]+\/[\w.-]+$/);
  assert.equal(endpoint.pathname, `/${repository}/releases/latest/download/latest.json`);
  return { config, version, publicKeyBytes, key, endpoint, repository };
}

export function expectedReleaseAssets(version, { installerOnly = false } = {}) {
  assert.match(version, /^\d+\.\d+\.\d+$/);
  const installer = `NetVision.Pro_${version}_x64-setup.exe`;
  const archives = installerOnly ? [] : [
    `NetVision.Pro_${version}_cli-windows-x64.zip`,
    `NetVision.Pro_${version}_cli-linux-x64.tar.gz`,
    `NetVision.Pro_${version}_cli-macos-x64.tar.gz`,
    `NetVision.Pro_${version}_cli-macos-arm64.tar.gz`,
  ];
  const binaries = [installer, ...archives];
  const names = ['latest.json', 'SHA256SUMS.txt',
    ...binaries.flatMap((filename) => [filename, `${filename}.sig`]),
    ...archives.map((filename) => `${filename}.sha256`)];
  return { installer, archives, binaries, names };
}

function verifyMinisign(data, encodedSignature, filename, sourceFilename, context) {
  const { version, publicKeyBytes, key } = context;
  const lines = decodeBase64(encodedSignature).toString('utf8').trimEnd().split(/\r?\n/);
  assert.equal(lines.length, 4, `${filename}: signature record must have four lines`);
  assert.ok(lines[0].startsWith('untrusted comment: '));
  const record = decodeBase64(lines[1]);
  assert.equal(record.length, 74);
  assert.equal(record.subarray(0, 2).toString('ascii'), 'ED', `${filename}: prehashed signature required`);
  assert.deepEqual(record.subarray(2, 10), publicKeyBytes.subarray(2, 10), `${filename}: updater public key ID`);
  assert.ok(lines[2].startsWith('trusted comment: '));
  const trustedComment = lines[2].slice('trusted comment: '.length);
  const signature = record.subarray(10);
  const hash = createHash('blake2b512').update(data).digest();
  assert.ok(verify(null, hash, key, signature), `${filename}: Ed25519 data signature`);
  const globalSignature = decodeBase64(lines[3]);
  assert.equal(globalSignature.length, 64);
  assert.ok(verify(null, Buffer.concat([signature, Buffer.from(trustedComment, 'utf8')]), key, globalSignature),
    `${filename}: trusted comment signature`);
  const fields = new Map();
  for (const field of trustedComment.split('\t')) {
    const colon = field.indexOf(':');
    assert.ok(colon > 0, `${filename}: trusted comment field`);
    const name = field.slice(0, colon);
    assert.ok(!fields.has(name), `${filename}: duplicate trusted comment field`);
    fields.set(name, field.slice(colon + 1));
  }
  assert.equal(fields.get('version'), version, `${filename}: signed version binding`);
  assert.equal(fields.get('file'), sourceFilename, `${filename}: signed filename binding`);
  assert.match(fields.get('timestamp') || '', /^\d+$/);
  return { filename, sha256: createHash('sha256').update(data).digest('hex'), signedVersion: version };
}

function parseChecksums(text) {
  const values = new Map();
  for (const line of text.trim().split(/\r?\n/)) {
    const match = line.match(/^([0-9a-f]{64}) [ *]([^/\\]+)$/i);
    assert.ok(match, 'Invalid SHA256 entry');
    assert.ok(!values.has(match[2]), `Duplicate SHA256 entry: ${match[2]}`);
    values.set(match[2], match[1].toLowerCase());
  }
  return values;
}

function verifyManifest(manifest, signature, context) {
  const { version, repository } = context;
  assert.equal(manifest.version, version, 'Updater manifest version');
  assert.ok(Number.isFinite(Date.parse(manifest.pub_date)), 'Valid updater publication date');
  const windows = manifest.platforms?.['windows-x86_64'];
  assert.ok(windows, 'Windows x64 updater platform exists');
  assert.equal(windows.url, `https://github.com/${repository}/releases/download/v${version}/NetVision.Pro_${version}_x64-setup.exe`);
  assert.equal(windows.signature, signature, 'Updater manifest signature binding');
}

export function verifyReleaseDirectory(directory, { config, expectedVersion, installerOnly = false }) {
  const context = releaseContext(config, expectedVersion);
  const assets = expectedReleaseAssets(context.version, { installerOnly });
  const entries = fs.readdirSync(directory, { withFileTypes: true });
  assert.ok(entries.every((entry) => entry.isFile()), 'Release directory contains only files');
  assert.deepEqual(entries.map((entry) => entry.name).sort(), [...assets.names].sort(),
    `Exactly ${assets.names.length} expected release assets`);
  const read = (name) => fs.readFileSync(path.join(directory, name));
  const checksums = parseChecksums(read('SHA256SUMS.txt').toString('utf8'));
  assert.deepEqual([...checksums.keys()].sort(), [...assets.binaries].sort(), 'Combined checksum filenames');
  const results = assets.binaries.map((filename) => {
    const sourceFilename = filename === assets.installer ? `${config.productName}_${context.version}_x64-setup.exe` : filename;
    const result = verifyMinisign(read(filename), read(`${filename}.sig`).toString('utf8'), filename, sourceFilename, context);
    assert.equal(checksums.get(filename), result.sha256, `${filename}: combined SHA256SUMS`);
    if (assets.archives.includes(filename)) {
      const single = parseChecksums(read(`${filename}.sha256`).toString('utf8'));
      assert.equal(single.size, 1);
      assert.equal(single.get(filename), result.sha256, `${filename}: standalone SHA256`);
    }
    return result;
  });
  verifyManifest(JSON.parse(read('latest.json').toString('utf8')),
    read(`${assets.installer}.sig`).toString('utf8').trim(), context);
  return { version: context.version, assetCount: assets.names.length, updaterManifest: 'verified',
    signatureAlgorithm: 'Ed25519 over BLAKE2b-512, plus authenticated trusted comment', binaries: results };
}

const runGitHub = (args) => execFileSync('gh', args, {
  encoding: 'utf8', timeout: 180_000, maxBuffer: 2_000_000, windowsHide: true,
});

// Unlike Node 22 fetch, curl also honors existing HTTP(S)_PROXY settings.
const fetchPublicManifest = async (url) => {
  const output = execFileSync('curl', ['--fail', '--location', '--silent', '--show-error',
    '--max-time', '30', '--connect-timeout', '10', '--proto', '=https', '--proto-redir', '=https',
    '--header', 'Accept: application/json', '--header', 'Cache-Control: no-cache',
    '--write-out', '\n%{http_code}', url],
    { encoding: 'utf8', timeout: 35_000, maxBuffer: 1_000_000, windowsHide: true });
  const separator = output.lastIndexOf('\n');
  return { status: Number(output.slice(separator + 1)), json: async () => JSON.parse(output.slice(0, separator)) };
};

function compareStableTags(left, right) {
  for (const value of [left, right]) assert.match(value, /^v\d+\.\d+\.\d+$/, 'GitHub latest must identify a stable release tag');
  const a = left.slice(1).split('.').map(BigInt);
  const b = right.slice(1).split('.').map(BigInt);
  for (let index = 0; index < a.length; index++) {
    if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  }
  return 0;
}

export async function verifyPublishedRelease(tag, {
  config, runGh = runGitHub, fetchImpl = fetchPublicManifest, sleep = delay,
  latestAttempts = 4, retryDelayMs = 5_000, installerOnly = false,
}) {
  assert.match(tag, /^v\d+\.\d+\.\d+$/, 'A stable release tag is required');
  assert.ok(Number.isInteger(latestAttempts) && latestAttempts >= 1 && latestAttempts <= 6);
  const context = releaseContext(config, tag);
  const repoArgs = ['--repo', context.repository];
  const release = JSON.parse(runGh(['release', 'view', tag, ...repoArgs, '--json', 'tagName,isDraft,isPrerelease,assets']));
  assert.equal(release.tagName, tag);
  assert.equal(release.isDraft, false, 'Release must be published');
  assert.equal(release.isPrerelease, false, 'Release must be stable');
  const assets = expectedReleaseAssets(context.version, { installerOnly });
  assert.deepEqual(release.assets.map((asset) => asset.name).sort(), [...assets.names].sort(), 'Published release asset set');
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'netvision-published-release-'));
  try {
    runGh(['release', 'download', tag, ...repoArgs, '--dir', temporary]);
    const result = verifyReleaseDirectory(temporary, { config, expectedVersion: tag, installerOnly });
    const signature = fs.readFileSync(path.join(temporary, `${assets.installer}.sig`), 'utf8').trim();
    let lastError;
    for (let attempt = 0; attempt < latestAttempts; attempt++) {
      try {
        const latest = JSON.parse(runGh(['release', 'view', ...repoArgs, '--json', 'tagName']));
        const order = compareStableTags(tag, latest.tagName);
        if (order < 0) return { ...result, latestEndpoint: 'skipped-historical-release' };
        assert.equal(order, 0, 'New release must become GitHub latest stable release');
        const response = await fetchImpl(context.endpoint.href, {
          signal: AbortSignal.timeout(30_000), cache: 'no-store', headers: { Accept: 'application/json' },
        });
        assert.equal(response.status, 200, 'Fixed updater endpoint HTTP status');
        verifyManifest(await response.json(), signature, context);
        return { ...result, latestEndpoint: 'verified' };
      } catch (error) {
        lastError = error;
        if (attempt + 1 < latestAttempts) await sleep(retryDelayMs);
      }
    }
    throw new Error(`Fixed updater endpoint failed after ${latestAttempts} attempts: ${lastError?.message}`, { cause: lastError });
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const config = JSON.parse(fs.readFileSync(path.resolve('src-tauri/tauri.conf.json'), 'utf8'));
  const args = process.argv.slice(2);
  try {
    let result;
    const installerOnlyFlags = args.filter((arg) => arg === '--installer-only').length;
    assert.ok(installerOnlyFlags <= 1, 'Do not repeat --installer-only');
    const installerOnly = installerOnlyFlags === 1;
    const positional = args.filter((arg) => arg !== '--installer-only');
    if (positional[0] === '--published') {
      assert.equal(positional.length, 2,
        'Usage: verify-public-release.mjs [directory] [version] [--installer-only] | --published vX.Y.Z [--installer-only]');
      result = await verifyPublishedRelease(positional[1], { config, installerOnly });
    } else {
      assert.ok(positional.length <= 2 && positional.every((arg) => !arg.startsWith('--')),
        'Usage: verify-public-release.mjs [directory] [version] [--installer-only] | --published vX.Y.Z [--installer-only]');
      result = verifyReleaseDirectory(path.resolve(positional[0] || 'release-artifacts'), {
        config, expectedVersion: positional[1], installerOnly,
      });
    }
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    console.error(`Release verification failed: ${error.message}`);
    process.exitCode = 1;
  }
}
