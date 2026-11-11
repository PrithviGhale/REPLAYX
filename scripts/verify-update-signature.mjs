// Verifies a Tauri updater signature (minisign, Ed25519) against REPLAYX's
// public key, independently of the app: the release workflow runs it on the
// built installer before a release is drafted, and it can be run by hand on
// any downloaded installer and its .sig file.
//
//   node scripts/verify-update-signature.mjs <installer.exe> <installer.exe.sig> [--version 0.3.2] [--pubkey <base64>]
//
// Without --pubkey the key built into the app is used (src-tauri/tauri.conf.json).
// With --version the signature must have been made for exactly that version
// (the app requires this too: requireSignedVersion).
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const text = b64 => Buffer.from(b64.trim(), 'base64').toString('utf8');
const ed25519 = raw => crypto.createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw]), format: 'der', type: 'spki' });

/** Throws with a reason when the signature is not valid; returns details when it is. */
export function verifyUpdate({ data, signature, pubkey, version }) {
  const keyLines = text(pubkey).split('\n').map(l => l.trim()).filter(Boolean);
  const key = Buffer.from(keyLines[keyLines.length - 1], 'base64');
  if (key.length !== 42 || key.subarray(0, 2).toString() !== 'Ed') throw new Error('The public key is not a minisign Ed25519 key');
  const [keyId, publicKey] = [key.subarray(2, 10), ed25519(key.subarray(10))];

  const lines = text(signature).split('\n').map(l => l.replace(/\r$/, ''));
  if (lines.length < 4 || !lines[2].startsWith('trusted comment: ')) throw new Error('The signature file is not in minisign format');
  const sig = Buffer.from(lines[1], 'base64');
  const trusted = lines[2].slice('trusted comment: '.length);
  const global = Buffer.from(lines[3], 'base64');
  if (sig.length !== 74 || global.length !== 64) throw new Error('The signature has the wrong length');
  const algorithm = sig.subarray(0, 2).toString();
  if (!sig.subarray(2, 10).equals(keyId)) throw new Error('The signature was made with a different key');
  // "ED": the BLAKE2b-512 hash of the file is signed; "Ed" (legacy): the file itself.
  const message = algorithm === 'ED' ? crypto.createHash('blake2b512').update(data).digest() : algorithm === 'Ed' ? data : null;
  if (!message) throw new Error(`Unknown signature algorithm ${algorithm}`);
  if (!crypto.verify(null, message, publicKey, sig.subarray(10))) throw new Error('The file does not match its signature');
  // The trusted comment (file name, version) is covered by the second signature.
  if (!crypto.verify(null, Buffer.concat([sig.subarray(10), Buffer.from(trusted, 'utf8')]), publicKey, global)) throw new Error('The signed comment was altered');
  const fields = Object.fromEntries(trusted.split('\t').map(f => { const i = f.indexOf(':'); return [f.slice(0, i), f.slice(i + 1)]; }));
  if (version !== undefined) {
    if (!fields.version) throw new Error('The signature does not name a version');
    if (fields.version.replace(/^v/, '') !== String(version).replace(/^v/, '')) throw new Error(`The signature is for version ${fields.version}, not ${version}`);
  }
  return { algorithm, file: fields.file ?? null, version: fields.version ?? null };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const option = name => (args.includes(name) ? args.splice(args.indexOf(name), 2)[1] : undefined);
  const [version, pubkeyArg] = [option('--version'), option('--pubkey')];
  const [file, sigFile] = args;
  if (!file || !sigFile) { console.error('usage: node scripts/verify-update-signature.mjs <file> <file.sig> [--version X] [--pubkey BASE64]'); process.exit(2); }
  const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
  const pubkey = pubkeyArg ?? JSON.parse(fs.readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).plugins.updater.pubkey;
  try {
    const result = verifyUpdate({ data: fs.readFileSync(file), signature: fs.readFileSync(sigFile, 'utf8'), pubkey, version });
    console.log(`OK: ${path.basename(file)} is signed by REPLAYX's update key${result.version ? ` for version ${result.version}` : ''}.`);
  } catch (e) {
    console.error(`INVALID: ${path.basename(file)}: ${e.message}`);
    process.exit(1);
  }
}
