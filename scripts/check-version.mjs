import fs from 'node:fs';
const version = JSON.parse(fs.readFileSync('package.json')).version;
const lock = JSON.parse(fs.readFileSync('package-lock.json'));
const versions = [version, lock.version, lock.packages[''].version,
  JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json')).version,
  fs.readFileSync('src-tauri/Cargo.toml','utf8').match(/^version = "([^"]+)"/m)?.[1],
  fs.readFileSync('src-tauri/Cargo.lock','utf8').match(/name = "netvision-pro"\r?\nversion = "([^"]+)"/)?.[1]];
if (!/^\d+\.\d+\.\d+$/.test(version) || versions.some(v => v !== version)) throw new Error(`Version mismatch: ${versions}`);
if (process.env.GITHUB_REF_TYPE === 'tag' && process.env.GITHUB_REF_NAME !== `v${version}`) throw new Error('Tag/version mismatch');
console.log(`Unified version OK: ${version}`);
