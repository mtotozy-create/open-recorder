#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import process from "node:process";
import { rootDir } from "./release-recommendation.mjs";

const macReleaseTargets = ["aarch64-apple-darwin", "x86_64-apple-darwin"];

function run(command, args) {
  execFileSync(command, args, {
    cwd: rootDir,
    stdio: "inherit",
  });
}

if (process.platform === "darwin") {
  console.log("Building macOS release bundles:");
  for (const target of macReleaseTargets) {
    console.log(`- ${target}`);
    run("tauri", ["build", "--target", target, "--bundles", "dmg"]);
  }
} else {
  console.log(`Using default Tauri release build for ${process.platform}`);
  run("tauri", ["build"]);
}
