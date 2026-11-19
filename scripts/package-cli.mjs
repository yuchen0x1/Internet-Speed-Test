import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
const target = process.argv[2];
if (!/^(windows-x64|linux-x64|macos-x64|macos-arm64)$/.test(target||'')) throw new Error('Explicit supported CLI target required');
const version = JSON.parse(fs.readFileSync('package.json')).version;
const executable = process.platform === 'win32' ? 'netvision-cli.exe' : 'netvision-cli';
const directory = path.resolve('release-artifacts');
fs.mkdirSync(directory,{recursive:true});
const stage = fs.mkdtempSync(path.join(directory,'stage-'));
try {
  fs.copyFileSync(`src-tauri/target/release/${executable}`,path.join(stage,executable));
  if (process.platform !== 'win32') fs.chmodSync(path.join(stage,executable),0o755);
  fs.copyFileSync('LICENSE',path.join(stage,'LICENSE'));
  fs.copyFileSync('docs/platforms.md',path.join(stage,'README.md'));
  const filename = `NetVision.Pro_${version}_cli-${target}.${process.platform==='win32'?'zip':'tar.gz'}`;
  const archive = path.join(directory,filename);
  const result = spawnSync('tar',[process.platform==='win32'?'-acf':'-czf',archive,'-C',stage,executable,'LICENSE','README.md'],{stdio:'inherit'});
  if (result.status!==0) throw new Error('CLI archive failed');
  fs.writeFileSync(archive+'.sha256',`${createHash('sha256').update(fs.readFileSync(archive)).digest('hex')}  ${filename}\n`);
  console.log(archive);
} finally { fs.rmSync(stage,{recursive:true,force:true}); }
