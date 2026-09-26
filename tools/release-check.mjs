import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const pkg = JSON.parse(read("package.json"));
const lock = JSON.parse(read("package-lock.json"));
const config = JSON.parse(read("src-tauri/tauri.conf.json"));
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
assert.equal(config.bundle.createUpdaterArtifacts, true);
assert.deepEqual(config.bundle.targets, ["nsis"]);
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
console.log(`Release v${pkg.version}: versions, signing key and updater endpoint agree.`);
