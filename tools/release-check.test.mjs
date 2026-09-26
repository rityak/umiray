import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

test("release checks accept the matching tag and reject a mismatched version", () => {
  const version = JSON.parse(readFileSync(new URL("../package.json", import.meta.url))).version;
  const run = (tag) =>
    spawnSync(process.execPath, [fileURLToPath(new URL("./release-check.mjs", import.meta.url))], {
      env: {
        ...process.env,
        GITHUB_REF_TYPE: "tag",
        GITHUB_REF_NAME: tag,
        GITHUB_REPOSITORY: "rityak/umiray",
      },
      encoding: "utf8",
    });
  expect(run(`v${version}`).status).toBe(0);
  const wrong = run("v999.0.0");
  expect(wrong.status).not.toBe(0);
  expect(wrong.stderr).toContain("Tag must match the version");
});
