import { spawnSync } from 'node:child_process';
const result = spawnSync(process.platform === 'win32' ? 'npm.cmd' : 'npm',
  ['audit', '--json', '--registry=https://registry.npmjs.org'],
  { encoding:'utf8', shell:process.platform === 'win32', maxBuffer:10_000_000 });
if (result.error || !result.stdout) throw new Error('npm audit failed to return JSON');
const report = JSON.parse(result.stdout);
if (report.error) throw new Error(JSON.stringify(report.error));
const vulnerabilities = report.vulnerabilities || {};
const expires = Date.parse('2026-12-19T00:00:00+08:00');
function reviewed(name, visited = new Set()) {
  if (visited.has(name)) return false;
  const v = vulnerabilities[name];
  if (!v || !v.isDirect && !v.via?.length) return false;
  if (!v.isDev && !v.dev) return false;
  const next = new Set(visited).add(name);
  return v.via.every(a => typeof a === 'string' ? reviewed(a, next)
    : a.url === 'https://github.com/advisories/GHSA-fc5q-39xc-vjpf' && Date.now() < expires);
}
// npm audit uses isDirect but does not expose dev classification in all versions.
// Determine dev-only membership from the lockfile, never assume tool names are safe.
const { readFileSync } = await import('node:fs');
const lock = JSON.parse(readFileSync('package-lock.json','utf8'));
for (const v of Object.values(vulnerabilities)) {
  v.isDev = v.nodes?.length > 0 && v.nodes.every(n => lock.packages[n]?.dev === true);
}
const rejected = Object.keys(vulnerabilities).filter(n => !reviewed(n));
for (const name of Object.keys(vulnerabilities)) console.log(`${rejected.includes(name) ? 'BLOCK' : 'REVIEWED dev-only exception until 2026-12-18'}: ${name}`);
if (rejected.length || !report.metadata) process.exitCode = 1;
else console.log('Dependency policy passed; known upstream advisory remains documented in SECURITY.md.');
