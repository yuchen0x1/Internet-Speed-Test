import { spawnSync } from 'node:child_process';
const result = spawnSync(process.platform === 'win32' ? 'npm.cmd' : 'npm',
  ['audit', '--json', '--registry=https://registry.npmjs.org'],
  { encoding:'utf8', shell:process.platform === 'win32', maxBuffer:10_000_000 });
if (result.error || !result.stdout) throw new Error('npm audit failed to return JSON');
const report = JSON.parse(result.stdout);
if (report.error) throw new Error(JSON.stringify(report.error));
if (!report.metadata) throw new Error('npm audit returned no metadata');
const vulnerabilities = Object.keys(report.vulnerabilities || {});
for (const name of vulnerabilities) console.log(`BLOCK: ${name}`);
if (vulnerabilities.length) process.exitCode = 1;
else console.log('Dependency policy passed: no known vulnerabilities.');
