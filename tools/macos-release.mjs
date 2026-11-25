// macOS updater package (D-149): the signed app tarball joins the shared latest.json.
//
// node tools/macos-release.mjs <version> <dir>
//   <dir> holds latest.json, umiray.app.tar.gz and umiray.app.tar.gz.sig.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "https://github.com/rityak/umiray";
const ASSET = "umiray.app.tar.gz";

export function withMacos(latest, version, signature) {
  if (latest.version?.replace(/^v/, "") !== version)
    throw new Error(`latest.json is for ${latest.version}, not ${version}`);
  return {
    ...latest,
    platforms: {
      ...latest.platforms,
      "darwin-aarch64": {
        signature: signature.trim(),
        url: `${REPO}/releases/download/v${version}/${ASSET}`,
      },
    },
  };
}

function main(version, dir) {
  const latest = withMacos(
    JSON.parse(readFileSync(join(dir, "latest.json"), "utf8")),
    version,
    readFileSync(join(dir, `${ASSET}.sig`), "utf8"),
  );
  writeFileSync(join(dir, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);
  console.log(`latest.json: darwin-aarch64 → ${latest.platforms["darwin-aarch64"].url}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, dir] = process.argv.slice(2);
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "") || !dir)
    throw new Error("usage: node tools/macos-release.mjs <X.Y.Z> <dir>");
  main(version, dir);
}
