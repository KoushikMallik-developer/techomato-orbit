// Fails unless a release tag (vX.Y.Z) matches every place the app's version is written.
import { readFileSync } from 'node:fs';

const tag = process.argv[2] ?? '';
// Plain X.Y.Z only: the MSI builder rejects non-numeric pre-release parts.
const m = /^v(\d+\.\d+\.\d+)$/.exec(tag);
if (!m) {
  console.error(`"${tag}" is not a release tag like v1.2.3`);
  process.exit(1);
}
const want = m[1];
const read = (p) => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');
const cargo = /^version\s*=\s*"([^"]+)"/m.exec(read('src-tauri/Cargo.toml'))?.[1];
const found = {
  'package.json': JSON.parse(read('package.json')).version,
  'src-tauri/tauri.conf.json': JSON.parse(read('src-tauri/tauri.conf.json')).version,
  'src-tauri/Cargo.toml': cargo,
};
const bad = Object.entries(found).filter(([, v]) => v !== want);
for (const [file, v] of bad) console.error(`${file} says ${v}, the tag says ${want}`);
if (bad.length) process.exit(1);
console.log(`Version ${want} matches everywhere.`);
