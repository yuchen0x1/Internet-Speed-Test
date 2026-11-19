import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
const executable = path.resolve(process.argv[2] || `src-tauri/target/release/netvision-cli${process.platform==='win32'?'.exe':''}`);
const directory = mkdtempSync(path.join(tmpdir(), 'netvision-cli-smoke-'));
function run(args) {
  const result = spawnSync(executable,args,{encoding:'utf8',env:{...process.env,NETVISION_DATA_DIR:directory},timeout:15000});
  if(result.error) throw result.error;
  return result;
}
try {
  assert.match(run(['--version']).stdout,/NetVision Pro CLI \d+\.\d+\.\d+/);
  assert.equal(run(['--help']).status,0);
  const history = run(['history','list','--json']);
  assert.equal(history.status,0,history.stderr);
  assert.deepEqual(JSON.parse(history.stdout),[]);
  const exported = run(['history','export','--format','json']);
  assert.equal(exported.status,0,exported.stderr);
  assert.deepEqual(JSON.parse(exported.stdout),[]);
  assert.equal(run(['speedtest','--invalid-argument']).status,1);
  console.log('CLI smoke PASS: version, help, isolated SQLite list/export, invalid arguments');
} finally { rmSync(directory,{recursive:true,force:true}); }
