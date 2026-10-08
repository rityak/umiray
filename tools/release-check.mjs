import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const pkg = JSON.parse(read("package.json"));
const lock = JSON.parse(read("package-lock.json"));
const config = JSON.parse(read("src-tauri/tauri.conf.json"));
const notes = read("CHANGELOG.md")
  .split(/^## /m)
  .find((section) => section.split(/\r?\n/, 1)[0] === pkg.version)
  ?.slice(pkg.version.length)
  .trim();
assert.ok(notes, `CHANGELOG.md must contain release notes for ${pkg.version}`);
for (const asset of ["public/ray-mark.svg", "public/ray.svg"])
  assert.match(read(asset), /<svg\b/, `${asset} must contain the app icon`);
assert.match(
  read(`src-tauri/${config.bundle.windows.nsis.installerHooks}`),
  /NSIS_HOOK_PREUNINSTALL/,
  "The uninstall hook must give Windows its proxy back (B-044)",
);
assert.match(pkg.version, /^\d+\.\d+\.\d+$/, "Release version must be stable X.Y.Z");
assert.equal(lock.version, pkg.version, "package-lock version differs");
assert.equal(lock.packages[""].version, pkg.version, "package-lock root version differs");
assert.equal(
  read("src-tauri/Cargo.toml").match(/^version = "([^"]+)"/m)?.[1],
  pkg.version,
  "Cargo version differs",
);
assert.ok(
  read("src-tauri/Cargo.lock").includes(`name = "umiray"\nversion = "${pkg.version}"`),
  "Cargo.lock version differs",
);
assert.equal(config.version, "../package.json");
assert.equal(config.mainBinaryName, "umiray");
assert.equal(config.bundle.createUpdaterArtifacts, true);
assert.deepEqual(config.bundle.targets, ["nsis"]);
// Linux (D-173): deb и rpm, помощник с правами назван polkit тем же путём, куда его кладёт пакет.
const linux = JSON.parse(read("src-tauri/tauri.linux.conf.json"));
assert.deepEqual(linux.bundle.targets, ["deb", "rpm"]);
const policy = "/usr/share/polkit-1/actions/com.umiray.client.policy";
for (const format of ["deb", "rpm"]) {
  const source = linux.bundle.linux[format].files[policy];
  assert.ok(source, `${format} must ship the polkit action`);
  assert.match(
    read(`src-tauri/${source}`),
    new RegExp(`exec\\.path">/usr/bin/${config.mainBinaryName}<`),
    "polkit must name the installed binary",
  );
}
assert.match(read("src-tauri/linux/arch/PKGBUILD"), /^pkgname=umiray-bin$/m);
assert.deepEqual(config.plugins.updater.endpoints, [
  "https://github.com/rityak/umiray/releases/latest/download/latest.json",
]);
assert.match(
  Buffer.from(config.plugins.updater.pubkey, "base64").toString(),
  /^untrusted comment:.*\nRW[^\n]+\s*$/,
);
if (process.env.GITHUB_REF_TYPE === "tag") {
  assert.equal(process.env.GITHUB_REF_NAME, `v${pkg.version}`, "Tag must match the version");
}
if (process.env.GITHUB_REPOSITORY)
  assert.equal(
    process.env.GITHUB_REPOSITORY,
    "rityak/umiray",
    "Updater endpoint belongs to another repository",
  );
console.log(
  process.argv.includes("--notes")
    ? notes
    : `Release v${pkg.version}: icons, versions, changelog, signing key and updater endpoint agree.`,
);
