import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { expectedReleaseAssets, verifyPublishedRelease, verifyReleaseDirectory } from '../verify-public-release.mjs';

const version = '1.2.3';
const tag = `v${version}`;
const sha256 = (data) => createHash('sha256').update(data).digest('hex');

function fixture(t, { installerOnly = false, signedVersion = version, signedFilename } = {}) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'netvision-release-fixture-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const keyId = randomBytes(8);
  const publicBytes = Buffer.concat([Buffer.from('Ed'), keyId, publicKey.export({ type: 'spki', format: 'der' }).subarray(-32)]);
  const pubkey = Buffer.from(`untrusted comment: test public key\n${publicBytes.toString('base64')}\n`).toString('base64');
  const config = { productName: 'NetVision Pro', version, plugins: { updater: {
    pubkey, endpoints: ['https://github.com/example/netvision/releases/latest/download/latest.json'],
  } } };
  const assets = expectedReleaseAssets(version, { installerOnly });
  const write = (name, value) => fs.writeFileSync(path.join(directory, name), value);
  const read = (name) => fs.readFileSync(path.join(directory, name));
  const signatures = new Map();
  const checksums = [];
  for (const filename of assets.binaries) {
    const data = Buffer.from(`Binary fixture for ${filename}`);
    write(filename, data);
    const signature = sign(null, createHash('blake2b512').update(data).digest(), privateKey);
    const sourceName = filename === assets.installer ? `NetVision Pro_${version}_x64-setup.exe` : filename;
    const comment = `timestamp:1795711300\tfile:${signedFilename || sourceName}\tversion:${signedVersion}`;
    const globalSignature = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
    const record = Buffer.concat([Buffer.from('ED'), keyId, signature]);
    const encoded = Buffer.from(`untrusted comment: test signature\n${record.toString('base64')}\ntrusted comment: ${comment}\n${globalSignature.toString('base64')}\n`).toString('base64');
    signatures.set(filename, encoded);
    write(`${filename}.sig`, encoded);
    const checksum = `${sha256(data)}  ${filename}\n`;
    checksums.push(checksum);
    if (assets.archives.includes(filename)) write(`${filename}.sha256`, checksum);
  }
  write('SHA256SUMS.txt', checksums.join(''));
  const manifest = { version, pub_date: '2026-11-27T00:00:00Z', notes: 'Test release', platforms: {
    'windows-x86_64': { signature: signatures.get(assets.installer),
      url: `https://github.com/example/netvision/releases/download/${tag}/${assets.installer}` },
  } };
  write('latest.json', JSON.stringify(manifest));
  return { directory, config, assets, manifest, write, read };
}

function publishedOptions(sample, { latestTag = tag, fetchImpl } = {}) {
  const downloads = [];
  const runGh = (args) => {
    assert.ok(args.includes('--repo') && args.includes('example/netvision'));
    if (args[1] === 'download') {
      const destination = args[args.indexOf('--dir') + 1];
      downloads.push(destination);
      fs.cpSync(sample.directory, destination, { recursive: true });
      return '';
    }
    if (args[2] === tag) return JSON.stringify({ tagName: tag, isDraft: false, isPrerelease: false,
      assets: sample.assets.names.map((name) => ({ name })) });
    return JSON.stringify({ tagName: latestTag });
  };
  return { downloads, options: { config: sample.config, runGh,
    fetchImpl: fetchImpl || (async () => ({ status: 200, json: async () => sample.manifest })),
    sleep: async () => {}, latestAttempts: 2, retryDelayMs: 0 } };
}

test('accepts all 16 assets with real Ed25519 data and trusted-comment signatures', (t) => {
  const sample = fixture(t);
  const result = verifyReleaseDirectory(sample.directory, { config: sample.config });
  assert.equal(result.assetCount, 16);
  assert.equal(result.binaries.length, 5);
  assert.equal(result.version, version);
});

test('uses explicit historical version instead of current configuration version', (t) => {
  const sample = fixture(t);
  sample.config.version = '9.9.9';
  assert.equal(verifyReleaseDirectory(sample.directory, { config: sample.config, expectedVersion: tag }).version, version);
});

test('rejects tampered binary content before accepting its checksums', (t) => {
  const sample = fixture(t);
  sample.write(sample.assets.installer, 'changed binary');
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /Ed25519 data signature/);
});

test('rejects tampered authenticated signature metadata', (t) => {
  const sample = fixture(t);
  const filename = `${sample.assets.installer}.sig`;
  const text = Buffer.from(sample.read(filename).toString(), 'base64').toString().replace('timestamp:1795711300', 'timestamp:1795711301');
  sample.write(filename, Buffer.from(text).toString('base64'));
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /trusted comment signature/);
});

test('rejects valid signatures bound to a different version or filename', (t) => {
  const old = fixture(t, { signedVersion: '1.2.2' });
  assert.throws(() => verifyReleaseDirectory(old.directory, { config: old.config }), /signed version binding/);
  const renamed = fixture(t, { signedFilename: 'other.exe' });
  assert.throws(() => verifyReleaseDirectory(renamed.directory, { config: renamed.config }), /signed filename binding/);
});

test('rejects a valid release signed by a different updater key', (t) => {
  const sample = fixture(t);
  const other = fixture(t);
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: other.config }), /updater public key ID/);
});

test('rejects missing or unexpected assets', (t) => {
  const sample = fixture(t);
  sample.write('unexpected.txt', 'extra');
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /Exactly 16/);
  fs.unlinkSync(path.join(sample.directory, 'unexpected.txt'));
  fs.unlinkSync(path.join(sample.directory, sample.assets.archives[0]));
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /Exactly 16/);
});

test('rejects wrong or duplicate checksums and altered updater download binding', (t) => {
  const sample = fixture(t);
  const sums = sample.read('SHA256SUMS.txt');
  sample.write('SHA256SUMS.txt', sums.toString().replace(/^[a-f0-9]{64}/, '0'.repeat(64)));
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /combined SHA256SUMS/);
  sample.write('SHA256SUMS.txt', Buffer.concat([sums, sums]));
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /Duplicate SHA256/);
  sample.write('SHA256SUMS.txt', sums);
  sample.manifest.platforms['windows-x86_64'].url = 'https://example.com/untrusted.exe';
  sample.write('latest.json', JSON.stringify(sample.manifest));
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /https:\/\/github.com/);
});

test('checks the four locally rebuilt installer assets without weakening the full-release default', (t) => {
  const sample = fixture(t, { installerOnly: true });
  assert.equal(verifyReleaseDirectory(sample.directory, { config: sample.config, installerOnly: true }).assetCount, 4);
  assert.throws(() => verifyReleaseDirectory(sample.directory, { config: sample.config }), /Exactly 16/);
});

test('downloads and verifies a published release and its fixed updater endpoint', async (t) => {
  const sample = fixture(t);
  let endpointCalls = 0;
  const { options, downloads } = publishedOptions(sample, { fetchImpl: async (url, request) => {
    endpointCalls++;
    assert.equal(url, sample.config.plugins.updater.endpoints[0]);
    assert.equal(request.cache, 'no-store');
    assert.ok(request.signal instanceof AbortSignal);
    return { status: 200, json: async () => sample.manifest };
  } });
  const result = await verifyPublishedRelease(tag, options);
  assert.equal(result.latestEndpoint, 'verified');
  assert.equal(endpointCalls, 1);
  assert.equal(downloads.length, 1);
  assert.equal(fs.existsSync(downloads[0]), false);
});

test('historical release repair verifies assets but does not require latest to point backwards', async (t) => {
  const sample = fixture(t);
  const { options } = publishedOptions(sample, { latestTag: 'v2.0.0', fetchImpl: async () => assert.fail('Must not fetch latest for historical release') });
  const result = await verifyPublishedRelease(tag, options);
  assert.equal(result.latestEndpoint, 'skipped-historical-release');
  assert.equal(result.assetCount, 16);
});

test('a newly published version cannot be mistaken for historical when latest still points to an older version', async (t) => {
  const sample = fixture(t);
  const { options, downloads } = publishedOptions(sample, { latestTag: 'v1.2.2',
    fetchImpl: async () => assert.fail('An older GitHub latest must be rejected before reading its manifest') });
  await assert.rejects(verifyPublishedRelease(tag, options), /New release must become GitHub latest stable release/);
  assert.equal(fs.existsSync(downloads[0]), false);
});

test('retries a stale latest manifest only up to the configured bound and cleans temporary downloads', async (t) => {
  const sample = fixture(t);
  let calls = 0;
  const { options, downloads } = publishedOptions(sample, { fetchImpl: async () => {
    calls++;
    return { status: 200, json: async () => ({ ...sample.manifest, version: '1.2.2' }) };
  } });
  await assert.rejects(verifyPublishedRelease(tag, options), /failed after 2 attempts/);
  assert.equal(calls, 2);
  assert.equal(fs.existsSync(downloads[0]), false);
});

test('accepts a latest endpoint that catches up within the bounded retry window', async (t) => {
  const sample = fixture(t);
  let calls = 0;
  const { options } = publishedOptions(sample, { fetchImpl: async () => {
    calls++;
    return { status: calls === 1 ? 503 : 200, json: async () => sample.manifest };
  } });
  assert.equal((await verifyPublishedRelease(tag, options)).latestEndpoint, 'verified');
  assert.equal(calls, 2);
});

test('rejects incomplete public releases before downloading files', async (t) => {
  const sample = fixture(t);
  const { options } = publishedOptions(sample);
  options.runGh = () => JSON.stringify({ tagName: tag, isDraft: false, isPrerelease: false, assets: [] });
  await assert.rejects(verifyPublishedRelease(tag, options), /Published release asset set/);
});
