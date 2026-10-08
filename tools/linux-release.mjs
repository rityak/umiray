// Linux release assets (D-173): deb and rpm join the Windows `latest.json`, and the Arch
// PKGBUILD gets this release's version and checksum.
//
// tauri-action writes the updater manifest for the Windows job only; the Linux job uploads its
// packages and signatures next to it, and this script adds them under the keys the updater asks
// for (`linux-x86_64-deb`, `linux-x86_64-rpm` — tauri-plugin-updater `get_urls`).
//
// node tools/linux-release.mjs <version> <dir>
//   <dir> holds latest.json from the release, the .deb, the .rpm and their .sig files.
//   Writes back latest.json, and writes PKGBUILD and .SRCINFO for the AUR.

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "https://github.com/rityak/umiray";
const TEMPLATE = new URL("../src-tauri/linux/arch/PKGBUILD", import.meta.url);
/// Scripts the PKGBUILD names (`install=`): they travel with it to the release and the AUR.
const INSTALL = new URL("../src-tauri/linux/arch/umiray.install", import.meta.url);

/// The updater key for each package format.
const FORMATS = [
  { key: "linux-x86_64-deb", file: (version) => `umiray_${version}_amd64.deb` },
  { key: "linux-x86_64-rpm", file: (version) => `umiray-${version}-1.x86_64.rpm` },
];

/// `latest.json` with the Linux packages added. The signature is the text of the .sig file,
/// as tauri-action writes it for Windows.
export function withLinux(latest, version, signatureOf) {
  if (latest.version?.replace(/^v/, "") !== version)
    throw new Error(`latest.json is for ${latest.version}, not ${version}`);
  const platforms = { ...latest.platforms };
  for (const { key, file } of FORMATS) {
    const name = file(version);
    platforms[key] = {
      signature: signatureOf(name).trim(),
      url: `${REPO}/releases/download/v${version}/${name}`,
    };
  }
  return { ...latest, platforms };
}

/// The template with this release's version and the deb's checksum.
export function pkgbuild(template, version, sha256) {
  const out = template
    .replace(/^pkgver=.*$/m, `pkgver=${version}`)
    .replace(/^pkgrel=.*$/m, "pkgrel=1")
    .replace(/^sha256sums=.*$/m, `sha256sums=('${sha256}')`);
  if (!out.includes(`pkgver=${version}`) || !out.includes(sha256))
    throw new Error("PKGBUILD template lost pkgver or sha256sums");
  return out;
}

/// `.SRCINFO` — what `makepkg --printsrcinfo` prints, for the simple PKGBUILD we ship:
/// scalar fields and quoted arrays, `${pkgver}` and `${url}` expanded.
export function srcinfo(text) {
  const fields = new Map();
  for (const [, key, value] of text.matchAll(/^(\w+)=(\([^)]*\)|.*)$/gm)) {
    const values = value.startsWith("(")
      ? [...value.matchAll(/'([^']*)'|"([^"]*)"/g)].map((m) => m[1] ?? m[2])
      : [value.replace(/^["']|["']$/g, "")];
    fields.set(key, values);
  }
  const one = (key) => fields.get(key)?.[0] ?? "";
  // Bash variables of the PKGBUILD itself, as makepkg expands them.
  const expand = (value) => value.replace(/\$\{(\w+)\}/g, (_, name) => one(name));
  const lines = [`pkgbase = ${one("pkgname")}`];
  const scalar = ["pkgdesc", "pkgver", "pkgrel", "url", "install"];
  const arrays = ["arch", "license", "depends", "optdepends", "provides", "conflicts", "options"];
  for (const key of scalar) if (fields.has(key)) lines.push(`\t${key} = ${one(key)}`);
  for (const key of arrays)
    for (const value of fields.get(key) ?? []) lines.push(`\t${key} = ${value}`);
  for (const value of fields.get("source") ?? []) lines.push(`\tsource = ${expand(value)}`);
  for (const value of fields.get("sha256sums") ?? []) lines.push(`\tsha256sums = ${value}`);
  lines.push("", `pkgname = ${one("pkgname")}`, "");
  return lines.join("\n");
}

function main(version, dir) {
  const read = (name) => readFileSync(join(dir, name), "utf8");
  const latest = withLinux(JSON.parse(read("latest.json")), version, (name) => read(`${name}.sig`));
  writeFileSync(join(dir, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);
  const deb = FORMATS[0].file(version);
  const sha256 = createHash("sha256")
    .update(readFileSync(join(dir, deb)))
    .digest("hex");
  const build = pkgbuild(readFileSync(TEMPLATE, "utf8"), version, sha256);
  writeFileSync(join(dir, "PKGBUILD"), build);
  writeFileSync(join(dir, ".SRCINFO"), srcinfo(build));
  writeFileSync(join(dir, "umiray.install"), readFileSync(INSTALL, "utf8"));
  console.log(`latest.json: ${Object.keys(latest.platforms).join(", ")}`);
  console.log(`PKGBUILD: umiray-bin ${version}, ${deb} sha256 ${sha256}`);
  console.log(`in ${dir}: ${readdirSync(dir).join(" ")}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, dir] = process.argv.slice(2);
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "") || !dir)
    throw new Error("usage: node tools/linux-release.mjs <X.Y.Z> <dir>");
  main(version, dir);
}
