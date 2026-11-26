import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { checkDependencyPatches, hashPatchFile, hashPatchTree, PATCH_POLICY } from '../check-dependency-patches.mjs';

const glibSource = `impl<'a> VariantStrIter<'a> {
    fn impl_get(&self, i: usize) -> &'a str {
        unsafe {
            let mut p: *mut libc::c_char = std::ptr::null_mut();
            ffi::g_variant_get_child(
                self.variant.to_glib_none().0,
                i,
                s as *const u8 as *const _,
                &mut p,
                std::ptr::null::<i8>(),
            );
        }
    }
}
#[test]
fn variant_str_iter_mutable_out_pointer_regression() {}
`;

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'netvision-patch-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  function write(filename, text) {
    const target = path.join(root, filename);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, text);
  }
  const policy = PATCH_POLICY.map(({ name, version, directory, semanticCheck }) => ({ name, version, directory, semanticCheck, files: {} }));
  write('src-tauri/Cargo.toml', `[package]\nname = "fixture"\nversion = "1.0.0"\n\n[patch.crates-io]\n${policy.map((p) => `${p.name} = { path = "../${p.directory}" }`).join('\n')}\n`);
  write('src-tauri/Cargo.lock', `version = 4\n${policy.map((p) => `\n[[package]]\nname = "${p.name}"\nversion = "${p.version}"\n`).join('')}`);
  for (const patch of policy) {
    write(`${patch.directory}/Cargo.toml`, `[package]\nname = "${patch.name}"\nversion = "${patch.version}"\n`);
    write(`${patch.directory}/README.md`, 'Reviewed upstream provenance.\n');
    if (patch.name === 'glib') write(`${patch.directory}/src/variant_iter.rs`, glibSource);
    for (const filename of patch.name === 'glib' ? ['Cargo.toml', 'src/variant_iter.rs'] : ['Cargo.toml']) {
      patch.files[filename] = hashPatchFile(path.join(root, patch.directory, filename));
    }
    patch.tree = hashPatchTree(path.join(root, patch.directory)).digest;
  }
  function change(filename, transform) { write(filename, transform(fs.readFileSync(path.join(root, filename), 'utf8'))); }
  return { root, policy, write, change, verify: () => checkDependencyPatches(root, policy) };
}

test('all six reviewed path crates are selected and CRLF checkouts remain valid', (t) => {
  const f = fixture(t);
  assert.equal(f.verify().packages, 6);
  const glib = f.policy.find((p) => p.name === 'glib');
  f.change(`${glib.directory}/src/variant_iter.rs`, (text) => text.replace(/\n/g, '\r\n'));
  assert.equal(f.verify().packages, 6);
});

test('removing or disabling the Cargo patch section fails', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.toml', (text) => text.replace('[patch.crates-io]', '[metadata.disabled-patches]'));
  assert.throws(f.verify, /exactly one \[patch\.crates-io\]/);
});

test('a missing crate patch cannot be mistaken for a successful audit', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.toml', (text) => text.replace(/^glib = .*\n/m, ''));
  assert.throws(f.verify, /missing path patch/);
});

test('redirecting a patch to a different path fails before reading it', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.toml', (text) => text.replace('../vendor/glib-0.18.5', '../vendor/glib-old'));
  assert.throws(f.verify, /missing or changed for glib/);
});

test('a registry lock entry is rejected even while the local patch remains declared', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.lock', (text) => text.replace('name = "glib"\nversion = "0.18.5"',
    'name = "glib"\nversion = "0.18.5"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "old-registry-digest"'));
  assert.throws(f.verify, /select the local path patch for glib/);
});

test('git sources and ambiguous alternate lock syntax fail closed', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.lock', (text) => text.replace('name = "glib"\nversion = "0.18.5"',
    'name = "glib"\nversion = "0.18.5"\n"source" = "git+https://example.invalid/reverted"'));
  assert.throws(f.verify, /Unsupported Cargo.lock package field/);
});

test('a git lock entry cannot bypass a declared local path patch', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.lock', (text) => text.replace('name = "glib"\nversion = "0.18.5"',
    'name = "glib"\nversion = "0.18.5"\nsource = "git+https://example.invalid/reverted#123456"'));
  assert.throws(f.verify, /select the local path patch for glib/);
});

test('missing resolved local packages fail', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.lock', (text) => text.replace('\n[[package]]\nname = "glib"\nversion = "0.18.5"\n', ''));
  assert.throws(f.verify, /one resolved local glib/);
});

test('duplicate resolved local packages fail', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.lock', (text) => `${text}\n[[package]]\nname = "glib"\nversion = "0.18.5"\n`);
  assert.throws(f.verify, /one resolved local glib/);
});

test('changing the resolved patch version fails', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.lock', (text) => text.replace('name = "glib"\nversion = "0.18.5"', 'name = "glib"\nversion = "0.18.6"'));
  assert.throws(f.verify, /Unexpected patched version for glib/);
});

for (const name of ['chrono', 'proc-macro-error', 'proc-macro-error-attr', 'proc-macro-error2', 'proc-macro-error-attr2', 'zbus_names', 'zvariant', 'zvariant_derive']) {
  test(`retired ${name} cannot return through any transitive dependency`, (t) => {
    const f = fixture(t);
    f.change('src-tauri/Cargo.lock', (text) => `${text}\n[[package]]\nname = "${name}"\nversion = "1.0.0"\n`);
    assert.throws(f.verify, new RegExp(`Retired dependency returned to Cargo.lock: ${name}`));
  });
}

test('modified critical source fails its reviewed file digest', (t) => {
  const f = fixture(t);
  const glib = f.policy.find((p) => p.name === 'glib');
  f.change(`${glib.directory}/src/variant_iter.rs`, (text) => text.replace('&mut p,', '&p,'));
  assert.throws(f.verify, /Reviewed patch file changed: vendor\/glib-0\.18\.5\/src\/variant_iter\.rs/);
});

test('the actual glib mutable out-pointer fix is required even after a digest refresh', (t) => {
  const f = fixture(t);
  const glib = f.policy.find((p) => p.name === 'glib');
  const filename = path.join(f.root, glib.directory, 'src/variant_iter.rs');
  f.change(`${glib.directory}/src/variant_iter.rs`, (text) => text.replace('&mut p,', '&p,'));
  glib.files['src/variant_iter.rs'] = hashPatchFile(filename);
  glib.tree = hashPatchTree(path.dirname(path.dirname(filename))).digest;
  assert.throws(f.verify, /must pass &mut p/);
});

test('the glib out-pointer declaration must stay mutable after a digest refresh', (t) => {
  const f = fixture(t);
  const glib = f.policy.find((p) => p.name === 'glib');
  const filename = path.join(f.root, glib.directory, 'src/variant_iter.rs');
  f.change(`${glib.directory}/src/variant_iter.rs`, (text) => text.replace('let mut p:', 'let p:'));
  glib.files['src/variant_iter.rs'] = hashPatchFile(filename);
  glib.tree = hashPatchTree(path.dirname(path.dirname(filename))).digest;
  assert.throws(f.verify, /official mutable out-pointer fix/);
});

test('removing the glib regression test is rejected after a digest refresh', (t) => {
  const f = fixture(t);
  const glib = f.policy.find((p) => p.name === 'glib');
  const filename = path.join(f.root, glib.directory, 'src/variant_iter.rs');
  f.change(`${glib.directory}/src/variant_iter.rs`, (text) => text.replace('#[test]', '#[allow(dead_code)]'));
  glib.files['src/variant_iter.rs'] = hashPatchFile(filename);
  glib.tree = hashPatchTree(path.dirname(path.dirname(filename))).digest;
  assert.throws(f.verify, /regression test is missing/);
});

test('added noncritical files cannot bypass the complete source-tree digest', (t) => {
  const f = fixture(t);
  f.write(`${f.policy[0].directory}/src/unreviewed.rs`, 'pub fn bypass() {}\n');
  assert.throws(f.verify, /Reviewed source tree changed: vendor\/serde_with/);
});

test('changed provenance files cannot bypass the complete source-tree digest', (t) => {
  const f = fixture(t);
  f.change(`${f.policy[0].directory}/README.md`, () => 'Unreviewed replacement crate.\n');
  assert.throws(f.verify, /Reviewed source tree changed: vendor\/serde_with/);
});

test('only the generated lockfile at a vendored crate root is excluded from source hashes', (t) => {
  const f = fixture(t);
  f.write(`${f.policy[0].directory}/Cargo.lock`, '# Generated by standalone dependency tests.\n');
  assert.equal(f.verify().packages, 6);
  f.write(`${f.policy[0].directory}/src/Cargo.lock`, '# Unreviewed nested file.\n');
  assert.throws(f.verify, /Reviewed source tree changed/);
});

test('missing reviewed source files fail instead of silently skipping their hash', (t) => {
  const f = fixture(t);
  fs.unlinkSync(path.join(f.root, f.policy[0].directory, 'Cargo.toml'));
  assert.throws(f.verify, /ENOENT/);
});

test('directory links cannot redirect vendored source outside the reviewed tree', (t) => {
  const f = fixture(t);
  const outside = path.join(f.root, 'unreviewed-source');
  fs.mkdirSync(outside);
  fs.symlinkSync(outside, path.join(f.root, f.policy[0].directory, 'redirect'), 'junction');
  assert.throws(f.verify, /Symlinks are not allowed inside dependency patches/);
});

test('empty or incomplete reviewed hash policies fail instead of silently skipping verification', (t) => {
  const f = fixture(t);
  f.policy[0].tree = '';
  assert.throws(f.verify, /Missing reviewed source-tree hash/);
  assert.throws(() => checkDependencyPatches(f.root, []), /must not be empty/);
});

test('duplicate patch declarations are rejected', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.toml', (text) => `${text}glib = { path = "../vendor/glib-0.18.5" }\n`);
  assert.throws(f.verify, /Duplicate patch: glib/);
});

test('a remote patch declaration is not accepted as a local patch', (t) => {
  const f = fixture(t);
  f.change('src-tauri/Cargo.toml', (text) => text.replace('glib = { path = "../vendor/glib-0.18.5" }',
    'glib = { git = "https://example.invalid/unreviewed" }'));
  assert.throws(f.verify, /Unsupported patch declaration/);
});

test('the CLI rejects ignore flags instead of changing the reviewed policy', () => {
  const script = fileURLToPath(new URL('../check-dependency-patches.mjs', import.meta.url));
  const result = spawnSync(process.execPath, [script, '--ignore'], { encoding: 'utf8' });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /does not accept policy overrides or ignore flags/);
});
