import { test } from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
// @ts-expect-error plain JavaScript module without type declarations
import { verifyUpdate } from '../scripts/verify-update-signature.mjs';

/** A throwaway key and a signature in the exact format the Tauri CLI writes. */
function keypair() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');
  const raw = publicKey.export({ format: 'der', type: 'spki' }).subarray(-32);
  const id = crypto.randomBytes(8);
  const pubkey = Buffer.from(`untrusted comment: minisign public key: TEST\n${Buffer.concat([Buffer.from('Ed'), id, raw]).toString('base64')}\n`).toString('base64');
  const sign = (data: Buffer, trusted: string) => {
    const sig = crypto.sign(null, crypto.createHash('blake2b512').update(data).digest(), privateKey);
    const global = crypto.sign(null, Buffer.concat([sig, Buffer.from(trusted)]), privateKey);
    return Buffer.from(`untrusted comment: signature from tauri secret key\n${Buffer.concat([Buffer.from('ED'), id, sig]).toString('base64')}\ntrusted comment: ${trusted}\n${global.toString('base64')}\n`).toString('base64');
  };
  return { pubkey, sign };
}
const installer = Buffer.from('REPLAYX installer bytes '.repeat(500));
const comment = 'timestamp:1794300000\tfile:REPLAYX_0.3.2_x64-setup.exe\tversion:0.3.2';

test('a correctly signed update is accepted, for the version it was signed for', () => {
  const k = keypair();
  const signature = k.sign(installer, comment);
  assert.deepEqual(verifyUpdate({ data: installer, signature, pubkey: k.pubkey, version: '0.3.2' }), { algorithm: 'ED', file: 'REPLAYX_0.3.2_x64-setup.exe', version: '0.3.2' });
  assert.equal(verifyUpdate({ data: installer, signature, pubkey: k.pubkey, version: 'v0.3.2' }).version, '0.3.2');
});

test('a tampered installer is rejected', () => {
  const k = keypair();
  const signature = k.sign(installer, comment);
  const tampered = Buffer.from(installer); tampered[100] ^= 1;
  assert.throws(() => verifyUpdate({ data: tampered, signature, pubkey: k.pubkey }), /does not match its signature/);
  assert.throws(() => verifyUpdate({ data: Buffer.concat([installer, Buffer.from('x')]), signature, pubkey: k.pubkey }), /does not match/);
});

test('a signature from another key is rejected', () => {
  const [ours, theirs] = [keypair(), keypair()];
  assert.throws(() => verifyUpdate({ data: installer, signature: theirs.sign(installer, comment), pubkey: ours.pubkey }), /different key/);
});

test('an old, validly signed installer cannot be passed off as a newer version', () => {
  const k = keypair();
  const old = k.sign(installer, 'timestamp:1\tfile:REPLAYX_0.3.1_x64-setup.exe\tversion:0.3.1');
  assert.throws(() => verifyUpdate({ data: installer, signature: old, pubkey: k.pubkey, version: '0.3.2' }), /is for version 0\.3\.1, not 0\.3\.2/);
  const unversioned = k.sign(installer, 'timestamp:1\tfile:REPLAYX_x64-setup.exe');
  assert.throws(() => verifyUpdate({ data: installer, signature: unversioned, pubkey: k.pubkey, version: '0.3.2' }), /does not name a version/);
});

test('an edited signed comment is rejected', () => {
  const k = keypair();
  const lines = Buffer.from(k.sign(installer, comment), 'base64').toString().split('\n');
  lines[2] = lines[2].replace('version:0.3.2', 'version:9.9.9');
  const forged = Buffer.from(lines.join('\n')).toString('base64');
  assert.throws(() => verifyUpdate({ data: installer, signature: forged, pubkey: k.pubkey, version: '9.9.9' }), /signed comment was altered/);
});

test('garbage is rejected with a reason, never accepted', () => {
  const k = keypair();
  assert.throws(() => verifyUpdate({ data: installer, signature: Buffer.from('not a signature').toString('base64'), pubkey: k.pubkey }), /not in minisign format/);
  assert.throws(() => verifyUpdate({ data: installer, signature: k.sign(installer, comment), pubkey: Buffer.from('nope').toString('base64') }), /not a minisign Ed25519 key/);
});
