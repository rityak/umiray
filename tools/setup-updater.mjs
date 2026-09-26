import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const key = `${root}.release/updater.key`;
const configPath = `${root}src-tauri/tauri.conf.json`;
const config = JSON.parse(readFileSync(configPath, "utf8"));
mkdirSync(`${root}.release`, { recursive: true });
if (!existsSync(key)) {
  if (config.plugins.updater.pubkey) {
    throw new Error(
      "Restore .release/updater.key from backup. Do not replace the key of released clients.",
    );
  }
  const result = spawnSync(
    process.execPath,
    [
      `${root}node_modules/@tauri-apps/cli/tauri.js`,
      "signer",
      "generate",
      "--ci",
      "--write-keys",
      key,
    ],
    { cwd: root, encoding: "utf8" },
  );
  if (result.status !== 0) throw new Error("Could not generate the updater key. Run npm ci first.");
}
const pubkey = readFileSync(`${key}.pub`, "utf8").trim();
if (config.plugins.updater.pubkey && config.plugins.updater.pubkey !== pubkey) {
  throw new Error(
    "Local signing key differs from the configured public key. Restore the correct key.",
  );
}
config.plugins.updater.pubkey = pubkey;
writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`);
if (process.argv.includes("--github")) {
  const result = spawnSync(
    "gh",
    ["secret", "set", "TAURI_SIGNING_PRIVATE_KEY", "--repo", "rityak/umiray"],
    {
      cwd: root,
      input: readFileSync(key),
      stdio: ["pipe", "inherit", "inherit"],
    },
  );
  if (result.status !== 0)
    throw new Error("Secret upload failed. Authenticate with gh auth login and retry.");
}
console.log("Public key configured. Back up .release/updater.key; never commit it.");
console.log(
  "Upload its contents as TAURI_SIGNING_PRIVATE_KEY in rityak/umiray Actions secrets, or run npm run updater:setup -- --github.",
);
