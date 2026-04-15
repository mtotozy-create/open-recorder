#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { rootDir } from "./release-recommendation.mjs";

const macReleaseTargets = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
const targetSuffixMap = {
  "aarch64-apple-darwin": "aarch64",
  "x86_64-apple-darwin": "x64",
};
const tauriConfigPath = path.join(rootDir, "src-tauri", "tauri.conf.json");
const tauriBinaryPath = path.join(
  rootDir,
  "node_modules",
  ".bin",
  process.platform === "win32" ? "tauri.cmd" : "tauri"
);

function run(command, args) {
  execFileSync(command, args, {
    cwd: rootDir,
    stdio: "inherit",
  });
}

function getTauriConfig() {
  return JSON.parse(fs.readFileSync(tauriConfigPath, "utf8"));
}

function buildMacAppBundle(target) {
  run(tauriBinaryPath, ["build", "--target", target, "--bundles", "app"]);
}

function createMacDmg(target, productName, version) {
  const appName = `${productName}.app`;
  const targetBundleDir = path.join(
    rootDir,
    "src-tauri",
    "target",
    target,
    "release",
    "bundle"
  );
  const appPath = path.join(targetBundleDir, "macos", appName);

  if (!fs.existsSync(appPath)) {
    throw new Error(`Expected app bundle at ${appPath}`);
  }

  const dmgDir = path.join(targetBundleDir, "dmg");
  const dmgSuffix = targetSuffixMap[target] ?? target;
  const dmgName = `${productName}_${version}_${dmgSuffix}.dmg`;
  const dmgPath = path.join(dmgDir, dmgName);
  const stageDir = fs.mkdtempSync(path.join(os.tmpdir(), `open-recorder-dmg-${dmgSuffix}-`));
  const stageContentsDir = path.join(stageDir, "contents");
  const tempDmgPath = path.join(stageDir, `rw-${dmgName}`);

  fs.mkdirSync(dmgDir, { recursive: true });
  fs.mkdirSync(stageContentsDir, { recursive: true });
  fs.rmSync(dmgPath, { force: true });

  try {
    fs.cpSync(appPath, path.join(stageContentsDir, appName), { recursive: true });
    fs.symlinkSync("/Applications", path.join(stageContentsDir, "Applications"));

    // Use a plain compressed image and avoid Finder AppleScript customization.
    run("hdiutil", [
      "makehybrid",
      "-default-volume-name",
      productName,
      "-hfs",
      "-ov",
      "-o",
      tempDmgPath,
      stageContentsDir,
    ]);
    run("hdiutil", [
      "convert",
      "-format",
      "UDZO",
      "-imagekey",
      "zlib-level=9",
      "-ov",
      "-o",
      dmgPath,
      tempDmgPath,
    ]);
  } finally {
    fs.rmSync(stageDir, { recursive: true, force: true });
  }
}

if (process.platform === "darwin") {
  const tauriConfig = getTauriConfig();
  const productName = tauriConfig.productName;
  const version = tauriConfig.version;

  if (typeof productName !== "string" || productName.length === 0) {
    throw new Error(`Invalid productName in ${tauriConfigPath}`);
  }

  if (typeof version !== "string" || version.length === 0) {
    throw new Error(`Invalid version in ${tauriConfigPath}`);
  }

  console.log("Building macOS release bundles:");
  for (const target of macReleaseTargets) {
    console.log(`- ${target}`);
    buildMacAppBundle(target);
    createMacDmg(target, productName, version);
  }
} else {
  console.log(`Using default Tauri release build for ${process.platform}`);
  run(tauriBinaryPath, ["build"]);
}
