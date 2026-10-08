import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { pkgbuild, srcinfo, withLinux } from "./linux-release.mjs";

const template = readFileSync(new URL("../src-tauri/linux/arch/PKGBUILD", import.meta.url), "utf8");

test("deb and rpm join latest.json under the keys the updater asks for", () => {
  const windows = { url: "https://x/umiray_1.5.0_x64-setup.exe", signature: "W" };
  const latest = withLinux(
    { version: "1.5.0", platforms: { "windows-x86_64": windows } },
    "1.5.0",
    (name) => `sig of ${name}\n`,
  );
  expect(latest.platforms["windows-x86_64"]).toEqual(windows);
  expect(latest.platforms["linux-x86_64-deb"]).toEqual({
    signature: "sig of umiray_1.5.0_amd64.deb",
    url: "https://github.com/rityak/umiray/releases/download/v1.5.0/umiray_1.5.0_amd64.deb",
  });
  expect(latest.platforms["linux-x86_64-rpm"].url).toMatch(/umiray-1\.5\.0-1\.x86_64\.rpm$/);
  expect(() => withLinux({ version: "1.4.0", platforms: {} }, "1.5.0", () => "")).toThrow();
});

test("the PKGBUILD gets the release version and the deb checksum, and .SRCINFO agrees", () => {
  const sha = "a".repeat(64);
  const build = pkgbuild(template, "1.5.0", sha);
  expect(build).toMatch(/^pkgver=1\.5\.0$/m);
  expect(build).toContain(`sha256sums=('${sha}')`);
  const info = srcinfo(build);
  expect(info).toContain("pkgbase = umiray-bin");
  expect(info).toContain("\tpkgver = 1.5.0");
  expect(info).toContain("\tdepends = webkit2gtk-4.1");
  expect(info).toContain("\tdepends = zenity");
  expect(info).toContain("\tinstall = umiray.install");
  expect(info).toContain(
    "\tsource = umiray_1.5.0_amd64.deb::https://github.com/rityak/umiray/releases/download/v1.5.0/umiray_1.5.0_amd64.deb",
  );
  expect(info).toContain(`\tsha256sums = ${sha}`);
  expect(info.trimEnd().endsWith("pkgname = umiray-bin")).toBe(true);
});
