import assert from 'node:assert/strict';
import { isUtf8 } from 'node:buffer';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('..', import.meta.url));
const retiredPackages = ['chrono', 'proc-macro-error', 'proc-macro-error-attr', 'proc-macro-error2', 'proc-macro-error-attr2', 'zbus_names', 'zvariant', 'zvariant_derive'];

// Hashes cover reviewed patch contents, with CRLF normalized for Git checkouts.
// Updating a vendored crate requires reviewing both its patch and these hashes.
export const PATCH_POLICY = Object.freeze([
  { name: 'serde_with', version: '3.24.0', directory: 'vendor/serde_with',
    tree: '9a9a6855c6294c342d7dbd26c3951cb16a47fb9c6af6c31756dbec59dc558b3e', files: {
      'Cargo.toml': '56519b953d1596a300c4bb3157f3b4d80c1a4c24e9021c1a90839cdb418c1343',
      'src/lib.rs': '65805f98ad48316cefff7c8f843f4bdd476b30e8302e9efcc06f04f7921de349',
      'NETVISION-PATCH.md': 'a6383f1f6b908d6ac670a2bbc71c3c11885f8ce13d6a0b41c5edfe04f45007c7',
    } },
  { name: 'glib', version: '0.18.5', directory: 'vendor/glib-0.18.5',
    tree: '50f864a8aafa7448919a5e7500ad1d94463798b02bbeadca3769dbedd8ed6323', files: {
      'Cargo.toml': 'bcd52d812b4c111864ae8e88ba6c0a8311eb0cf781b5e81634950b4619592132',
      'src/variant_iter.rs': '210cb7c83b87d1a41c2010905e81500adec45a81fba78fb1bb940781bac6ff2b',
    }, semanticCheck: 'glib-variant-out-pointer' },
  { name: 'glib-macros', version: '0.18.5', directory: 'vendor/glib-macros-0.18.5',
    tree: 'f61f10d124e6ba6e7d5e2c28e82ac04c85800dcb442afaeabebf540fe4b3e0a2', files: {
      'Cargo.toml': 'f75de730d1675d3cb07b32ba65404956b2e7d9f9495940ef3964c906435ac477',
      'src/lib.rs': 'f05e0d5919280b09b27a9ec8ce1cdeab88f576f502e1a34201082f31b0d157ba',
      'src/boxed_derive.rs': 'b479bae1c5113c8ced199e885dd1162195e2308ae2a1aea23e42a0673a16ab00',
      'src/closure.rs': '4c08a1f77ae6abf2ba3b230e8a012ffc8be81ebad30a7ecc8c4836d3272826c4',
      'src/derived_properties_attribute.rs': '102215af444c0e2b4d1c82773d20d27c2c7a2ad489fdd1b671c75e438ec8b07d',
      'src/enum_derive.rs': '154642519b087551995768181fc0af3ee43715db0b8101aa6c424ad591ec2c50',
      'src/error_domain_derive.rs': 'b8b2912c96219a44db8da4c9ad8ff3d6e9d08aa0ab0845c9025540aae4e0fa40',
      'src/flags_attribute.rs': 'b66abc14da253fa61a52902bce38d44bca7a0ef419623c388453d0fe422325e1',
      'src/object_interface_attribute.rs': '7dc891269e554fda31812cb83c4c05fdd299fb2cacc2deb05ed37bb128907004',
      'src/object_subclass_attribute.rs': 'c6ce6e32594d59c94f55ba2d20d4f8bd1ce6a4117ad4581202636c36615ee868',
      'src/shared_boxed_derive.rs': '2bda2e3c90c36bdacc651b5f0f419c481bb521c9c26c3b4f7bd2a6c418ca3eea',
      'src/variant_derive.rs': '60af51e7e050765219e4b2552eb339fc97072dbe23b087de88365c2cf97728ca',
    } },
  { name: 'gtk3-macros', version: '0.18.2', directory: 'vendor/gtk3-macros-0.18.2',
    tree: 'e525c677b99e5920b719a642929ea1768eebd63deabf1aec5d3934ef243ae336', files: {
      'Cargo.toml': 'bcfb696934d5016f5ac09ed98890f0ed791d8cf411636c4b2af6b3920465bf45',
      'src/lib.rs': '6283f1e56c2dca0f4675bf1fb1b1c24a66128dd481abc97d52593dcfeb73186a',
      'src/composite_template_derive.rs': 'e982c44dc3c3f541e879e95193fb9711abf9624ca44d6879d413c4575687f84e',
    } },
  { name: 'tauri-plugin-notification', version: '2.8.1', directory: 'vendor/tauri-plugin-notification-2.8.1',
    tree: 'b5d76907dc402d1917e072dba24a36730d4516adbe3299bba6700b843adc6097', files: {
      'Cargo.toml': '3bddff7b8cd0630c87a53b44f857f21f417597f4c092234350577b35283bd660',
      'src/desktop.rs': 'bc1bc45994cbba2967adada7750d92399bf126b26aed2bb475546a55135983e7',
    } },
  { name: 'tauri-plugin-single-instance', version: '2.5.5', directory: 'vendor/tauri-plugin-single-instance-2.5.5',
    tree: 'e3218944619a12c3bbb8fee04640d267d2d36128c5c8560901df4b0a286c355f', files: {
      'Cargo.toml': 'a4ca97f2c0e20efe5f28e67d92cd611d0e35046d238628e790696b5988f93d1a',
      'src/platform_impl/linux.rs': 'd8c811c694c4a1a87343f8f7ac299c9c031d3491ce73fc29e1c4cedc41b89839',
      'tests/dbus_errors.rs': 'd8648dc2b4d0102a80f0e5bb831cca26b649a3ec577d83d4813f7981e0822382',
      'tests/instance_lock.rs': '6c2a23926b517f70e925a1df15a1b06228466df12a45b49347cd06f6228cbeb7',
    } },
]);

function readString(value, context) {
  let parsed;
  try { parsed = JSON.parse(value); } catch { throw new Error(`Unsupported Cargo string at ${context}`); }
  assert.equal(typeof parsed, 'string', `Expected Cargo string at ${context}`);
  return parsed;
}

function readPatches(manifest) {
  const patches = new Map();
  let inPatches = false;
  let patchTables = 0;
  for (const line of manifest.split(/\r?\n/)) {
    if (/^\s*\[/.test(line)) {
      inPatches = /^\s*\[patch\.crates-io\]\s*(?:#.*)?$/.test(line);
      if (inPatches) patchTables++;
      else assert.ok(!/^\s*\[patch[.\]]/.test(line), 'Unexpected patch table; review the dependency patch policy');
      continue;
    }
    if (!inPatches || /^\s*(?:#.*)?$/.test(line)) continue;
    const entry = /^\s*([\w-]+)\s*=\s*\{\s*path\s*=\s*("[^"\r\n]+")\s*\}\s*(?:#.*)?$/.exec(line);
    assert.ok(entry, `Unsupported patch declaration: ${line.trim()}`);
    assert.ok(!patches.has(entry[1]), `Duplicate patch: ${entry[1]}`);
    patches.set(entry[1], readString(entry[2], `patch.${entry[1]}.path`));
  }
  assert.equal(patchTables, 1, 'Expected exactly one [patch.crates-io] table');
  return patches;
}

// Cargo writes a stable TOML subset here. Reject unfamiliar syntax rather than
// overlooking an alternate source declaration or a package replacement.
function readLockPackages(lock) {
  const packages = [];
  let current;
  let dependencies = false;
  for (const [index, raw] of lock.split(/\r?\n/).entries()) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    if (line === '[[package]]') {
      assert.ok(!dependencies, 'Unclosed Cargo.lock dependency array');
      current = {};
      packages.push(current);
      continue;
    }
    if (line.startsWith('[')) {
      assert.ok(!dependencies, 'Unclosed Cargo.lock dependency array');
      current = undefined;
      continue;
    }
    if (!current) continue;
    if (dependencies) {
      if (line === ']') dependencies = false;
      else assert.match(line, /^"(?:[^"\\]|\\.)*",?$/, `Unsupported Cargo.lock dependency at line ${index + 1}`);
      continue;
    }
    const field = /^(name|version|source|checksum|dependencies)\s*=\s*(.*)$/.exec(line);
    assert.ok(field, `Unsupported Cargo.lock package field at line ${index + 1}`);
    assert.ok(!Object.hasOwn(current, field[1]), `Duplicate Cargo.lock ${field[1]} field`);
    if (field[1] === 'dependencies') {
      assert.ok(field[2] === '[' || field[2] === '[]', 'Unsupported Cargo.lock dependency list');
      current.dependencies = true;
      dependencies = field[2] === '[';
    } else current[field[1]] = readString(field[2], `Cargo.lock:${index + 1}`);
  }
  assert.ok(!dependencies, 'Unclosed Cargo.lock dependency array');
  for (const entry of packages) {
    assert.equal(typeof entry.name, 'string', 'Missing Cargo.lock package name');
    assert.equal(typeof entry.version, 'string', `Missing Cargo.lock version for ${entry.name}`);
  }
  assert.ok(packages.length > 0, 'Cargo.lock has no packages');
  return packages;
}

export function hashPatchFile(filename) {
  const bytes = fs.readFileSync(filename);
  const normalized = isUtf8(bytes) && !bytes.includes(0)
    ? bytes.toString('utf8').replace(/\r\n/g, '\n') : bytes;
  return createHash('sha256').update(normalized).digest('hex');
}

export function hashPatchTree(directory) {
  const files = [];
  function visit(relative = '') {
    for (const name of fs.readdirSync(path.join(directory, relative)).sort()) {
      // Standalone library tests create a private lockfile; the application
      // resolves all patched dependencies through src-tauri/Cargo.lock instead.
      if (relative === '' && name === 'Cargo.lock') continue;
      const child = path.posix.join(relative, name);
      const filename = path.join(directory, child);
      const info = fs.lstatSync(filename);
      assert.ok(!info.isSymbolicLink(), `Symlinks are not allowed inside dependency patches: ${filename}`);
      if (info.isDirectory()) {
        if (name !== 'target' && name !== '.git') visit(child);
      } else {
        assert.ok(info.isFile(), `Unexpected dependency patch entry: ${filename}`);
        files.push([child, hashPatchFile(filename)]);
      }
    }
  }
  visit();
  const hash = createHash('sha256');
  for (const [filename, digest] of files.sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) hash.update(`${filename}\0${digest}\n`);
  return { digest: hash.digest('hex'), files: files.length };
}

function checkGlibVariant(directory) {
  const source = fs.readFileSync(path.join(directory, 'src/variant_iter.rs'), 'utf8').replace(/\r\n/g, '\n');
  const getter = source.match(/fn impl_get\([^]*?\n {4}\}/)?.[0];
  assert.ok(getter, 'glib VariantStrIter getter is missing');
  assert.match(getter, /let mut p:\s*\*mut libc::c_char\s*=\s*std::ptr::null_mut\(\)/,
    'glib VariantStrIter must use the official mutable out-pointer fix');
  assert.match(getter, /ffi::g_variant_get_child\([^]*?\n\s*&mut p,/,
    'glib VariantStrIter must pass &mut p to the C out-parameter');
  assert.match(source, /#\[test\]\s*fn variant_str_iter_mutable_out_pointer_regression\(\)/,
    'glib optimized out-pointer regression test is missing');
}

export function checkDependencyPatches(root = repositoryRoot, policy = PATCH_POLICY) {
  assert.ok(policy.length > 0, 'Dependency patch policy must not be empty');
  const manifestPath = path.join(root, 'src-tauri/Cargo.toml');
  const patches = readPatches(fs.readFileSync(manifestPath, 'utf8'));
  assert.equal(patches.size, policy.length, 'Unexpected or missing path patch; review the dependency patch policy');
  const packages = readLockPackages(fs.readFileSync(path.join(root, 'src-tauri/Cargo.lock'), 'utf8'));
  for (const name of retiredPackages) assert.ok(!packages.some((entry) => entry.name === name), `Retired dependency returned to Cargo.lock: ${name}`);
  let files = 0;
  const rootPath = fs.realpathSync(root);
  for (const patch of policy) {
    const expectedPath = `../${patch.directory}`;
    assert.equal(patches.get(patch.name), expectedPath, `Path patch is missing or changed for ${patch.name}`);
    const directory = path.resolve(path.dirname(manifestPath), expectedPath);
    assert.equal(fs.realpathSync(directory), path.join(rootPath, patch.directory), `Path patch escapes the repository: ${patch.name}`);
    const selected = packages.filter((entry) => entry.name === patch.name);
    assert.equal(selected.length, 1, `Expected one resolved local ${patch.name} package`);
    assert.equal(selected[0].version, patch.version, `Unexpected patched version for ${patch.name}`);
    assert.ok(!Object.hasOwn(selected[0], 'source') && !Object.hasOwn(selected[0], 'checksum'),
      `Cargo.lock must select the local path patch for ${patch.name}, not a registry/git source`);
    assert.match(patch.tree, /^[a-f0-9]{64}$/, `Missing reviewed source-tree hash for ${patch.name}`);
    assert.ok(Object.hasOwn(patch.files, 'Cargo.toml'), `Manifest hash is missing for ${patch.name}`);
    for (const [filename, expected] of Object.entries(patch.files)) {
      assert.match(expected, /^[a-f0-9]{64}$/, `Missing reviewed SHA-256 for ${patch.name}/${filename}`);
      assert.equal(hashPatchFile(path.join(directory, filename)), expected,
        `Reviewed patch file changed: ${patch.directory}/${filename}`);
    }
    if (patch.semanticCheck === 'glib-variant-out-pointer') checkGlibVariant(directory);
    else assert.equal(patch.semanticCheck, undefined, `Unknown semantic patch check: ${patch.name}`);
    const tree = hashPatchTree(directory);
    assert.equal(tree.digest, patch.tree, `Reviewed source tree changed: ${patch.directory}; review added, removed, or modified files before updating the policy`);
    files += tree.files;
  }
  return { packages: policy.length, files };
}

if (process.argv[1] && fs.realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    assert.equal(process.argv.length, 2, 'This check does not accept policy overrides or ignore flags');
    const result = checkDependencyPatches();
    console.log(`Dependency patches verified: ${result.packages} resolved local crates, ${result.files} reviewed files; retired crates absent`);
  } catch (error) {
    console.error(`Dependency patch verification failed: ${error.message}`);
    process.exitCode = 1;
  }
}
