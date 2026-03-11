#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(scriptDir, "..");
const packageJsonPath = path.join(rootDir, "package.json");

function bumpSemver(version, bumpType) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    throw new Error(`Unsupported version format: ${version}`);
  }

  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);

  if (bumpType === "major") {
    return `${major + 1}.0.0`;
  }
  if (bumpType === "minor") {
    return `${major}.${minor + 1}.0`;
  }
  if (bumpType === "patch") {
    return `${major}.${minor}.${patch + 1}`;
  }

  throw new Error(`Unsupported bump type: ${bumpType}`);
}

function run(command, args) {
  return execFileSync(command, args, {
    cwd: rootDir,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

const pkg = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
const currentVersion = pkg.version;

if (typeof currentVersion !== "string" || currentVersion.length === 0) {
  throw new Error("package.json version is missing");
}

const recommendedBumpRaw = run("conventional-recommended-bump", [
  "-p",
  "conventionalcommits",
  "-t",
  "v",
]);
const recommendedBump = recommendedBumpRaw.toLowerCase();

if (!["major", "minor", "patch"].includes(recommendedBump)) {
  throw new Error(
    `Expected recommended bump to be major/minor/patch, got: ${recommendedBumpRaw}`
  );
}

const nextVersion = bumpSemver(currentVersion, recommendedBump);
const changelogPreview = run("conventional-changelog", [
  "-p",
  "conventionalcommits",
  "-t",
  "v",
  "-r",
  "1",
  "--stdout",
]);

console.log("Release Preview");
console.log(`Current version: ${currentVersion}`);
console.log(`Recommended bump: ${recommendedBump}`);
console.log(`Next version: ${nextVersion}`);
console.log("");
console.log("Changelog preview:");
console.log(changelogPreview);
