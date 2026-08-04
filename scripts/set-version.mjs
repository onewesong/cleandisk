#!/usr/bin/env node

import fs from "node:fs";

const packageJson = JSON.parse(fs.readFileSync("package.json", "utf8"));
const version = packageJson.version;

if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`package.json contains an invalid semantic version: ${version}`);
}

const tauriConfigPath = "src-tauri/tauri.conf.json";
const tauriConfig = JSON.parse(fs.readFileSync(tauriConfigPath, "utf8"));
tauriConfig.version = version;
fs.writeFileSync(tauriConfigPath, `${JSON.stringify(tauriConfig, null, 2)}\n`);

const cargoManifestPath = "src-tauri/Cargo.toml";
const cargoManifest = fs.readFileSync(cargoManifestPath, "utf8");
if (!/^(\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m.test(cargoManifest)) {
  throw new Error("Could not find the package version in src-tauri/Cargo.toml");
}
const updatedCargoManifest = cargoManifest.replace(
  /^(\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
  `$1"${version}"`,
);

fs.writeFileSync(cargoManifestPath, updatedCargoManifest);
console.log(`Synchronized Tauri and Cargo versions to ${version}.`);
