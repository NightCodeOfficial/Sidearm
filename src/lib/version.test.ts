import { describe, it, expect } from "vitest";
import packageJson from "../../package.json?raw";
import packageLock from "../../package-lock.json?raw";
import tauriConf from "../../src-tauri/tauri.conf.json?raw";
import cargoToml from "../../src-tauri/Cargo.toml?raw";

// The version lives in four files; the build banner reads package.json while the
// device report prints the crate's version. A release that bumps one and not the
// others ships two version numbers — fail CI instead.
describe("app version", () => {
  it("is the same in package.json, package-lock.json, tauri.conf.json and Cargo.toml", () => {
    const version = (JSON.parse(packageJson) as { version: string }).version;
    const lock = JSON.parse(packageLock) as { version: string; packages: Record<string, { version?: string }> };
    const crate = /^\[package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
    expect({
      lock: lock.version,
      lockRoot: lock.packages[""]?.version,
      tauri: (JSON.parse(tauriConf) as { version: string }).version,
      crate,
    }).toEqual({ lock: version, lockRoot: version, tauri: version, crate: version });
  });
});
