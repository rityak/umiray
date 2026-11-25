import { expect, test } from "vitest";
import { withMacos } from "./macos-release.mjs";

test("the signed macOS package joins latest.json under the updater key", () => {
  const windows = { url: "https://x/umiray_1.5.0_x64-setup.exe", signature: "W" };
  const latest = withMacos(
    { version: "1.5.0", platforms: { "windows-x86_64": windows } },
    "1.5.0",
    "mac signature\n",
  );
  expect(latest.platforms["windows-x86_64"]).toEqual(windows);
  expect(latest.platforms["darwin-aarch64"]).toEqual({
    signature: "mac signature",
    url: "https://github.com/rityak/umiray/releases/download/v1.5.0/umiray.app.tar.gz",
  });
  expect(() => withMacos({ version: "1.4.0", platforms: {} }, "1.5.0", "")).toThrow();
});
