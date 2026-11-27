import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('..', import.meta.url));
const toolVersion = '8.30.1';
const toolArchive = `gitleaks_${toolVersion}_windows_x64.zip`;
const checksumsName = `gitleaks_${toolVersion}_checksums.txt`;
const toolDirectory = path.join(os.tmpdir(), 'netvision-gitleaks-cache');

const onPath = spawnSync('gitleaks', ['version'], { encoding: 'utf8', windowsHide: true });
if (!onPath.error && onPath.status === 0) {
  console.log(`Using installed ${onPath.stdout.trim() || 'gitleaks'}.`);
} else {
  const downloadDirectory = path.join(toolDirectory, toolVersion);
  fs.mkdirSync(downloadDirectory, { recursive: true });
  const executable = path.join(downloadDirectory, 'gitleaks.exe');
  const archive = path.join(downloadDirectory, toolArchive);
  if (!fs.existsSync(executable)) {
    const checksumFile = path.join(downloadDirectory, checksumsName);
    const runGh = (pattern) => {
      const result = spawnSync('gh', ['release', 'download', `v${toolVersion}`,
        '--repo', 'gitleaks/gitleaks', '--pattern', pattern, '--clobber', '--dir', downloadDirectory],
        { encoding: 'utf8', windowsHide: true, maxBuffer: 1_000_000 });
      assert.ok(!result.error && result.status === 0,
        `Unable to download ${pattern}: ${result.error?.message ?? result.stderr}`);
    };
    if (!fs.existsSync(checksumFile)) runGh(checksumsName);
    const digestLine = fs.readFileSync(checksumFile, 'utf8').split(/\r?\n/)
      .find((line) => line.endsWith(toolArchive));
    assert.ok(digestLine, `Missing checksum for ${toolArchive}`);
    const expectedDigest = digestLine.split(/\s+/)[0].toLowerCase();
    if (!fs.existsSync(archive)) runGh(toolArchive);
    const actualDigest = createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
    assert.equal(actualDigest, expectedDigest, 'Downloaded gitleaks archive failed SHA-256 verification');
    const expand = spawnSync('tar.exe', ['-xf', archive, '-C', downloadDirectory],
      { encoding: 'utf8', windowsHide: true });
    assert.ok(expand.status === 0, `Unable to unpack gitleaks: ${expand.stderr}`);
  }
  process.env.PATH = `${downloadDirectory}${path.delimiter}${process.env.PATH ?? ''}`;
  console.log(`Using verified gitleaks ${toolVersion}.`);
}

const redactedReport = path.join(os.tmpdir(), `netvision-gitleaks-${Date.now()}.json`);
const runScan = (command) => spawnSync('gitleaks', [...command, '--redact=100', '--no-banner',
  '--report-format', 'json', '--report-path', redactedReport],
  { encoding: 'utf8', windowsHide: true, maxBuffer: 5_000_000, cwd: repositoryRoot });
let scan = runScan(['git', '.']);
if (scan.status !== 0 && scan.status !== 1) scan = runScan(['dir', '.']);
fs.rmSync(redactedReport, { force: true });
if (scan.status === 0) {
  console.log('Gitleaks credential scan passed; no credentials found.');
  process.exit(0);
}
if (scan.status === 1) {
  console.error('Gitleaks found possible credentials in Git history. Details are redacted; review locally before proceeding.');
  process.exit(1);
}
assert.ok(!scan.error, `Gitleaks failed to run: ${scan.error?.message ?? ''}`);
throw new Error(scan.stderr || 'Gitleaks exited with an unexpected status');
