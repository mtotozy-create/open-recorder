#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import {
  bumpSemver,
  getCurrentVersion,
  getReleaseRecommendation,
  rootDir,
} from "./release-recommendation.mjs";
import { writeReleaseNotesFile } from "./release-notes.mjs";

const dmgOutputDir = path.join(
  rootDir,
  "src-tauri",
  "target",
  "release",
  "bundle",
  "dmg"
);

function findReleaseAssets(version) {
  if (!fs.existsSync(dmgOutputDir)) {
    return [];
  }

  const versionMarker = `_${version}_`;

  return fs
    .readdirSync(dmgOutputDir, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name)
    .filter(
      (fileName) =>
        fileName.endsWith(".dmg") &&
        (fileName.includes(versionMarker) || fileName.includes(`_${version}.dmg`))
    )
    .sort()
    .map((fileName) => path.join(dmgOutputDir, fileName));
}

const passthroughArgs = process.argv.slice(2);
const recommendation = getReleaseRecommendation();
const supportedBumps = new Set(["patch", "minor", "major"]);
const requestedBump = supportedBumps.has(passthroughArgs[0])
  ? passthroughArgs.shift()
  : null;
const releaseBump = requestedBump ?? recommendation.recommendedBump;
const nextVersion = bumpSemver(recommendation.currentVersion, releaseBump);

console.log("Auto Release");
console.log(`Base tag: ${recommendation.baseTag ?? "(none)"}`);
console.log(`Recommended bump: ${recommendation.recommendedBump}`);
if (requestedBump) {
  console.log(`Requested bump override: ${requestedBump}`);
}
console.log(`Next version: ${nextVersion}`);

for (const reason of recommendation.reasonLines) {
  console.log(`- ${reason}`);
}

execFileSync(
  "release-it",
  [releaseBump, "--no-github.release", ...passthroughArgs],
  {
    cwd: rootDir,
    stdio: "inherit",
  }
);

const releasedVersion = getCurrentVersion();
const releaseTag = `v${releasedVersion}`;
const notesPath = writeReleaseNotesFile(releasedVersion);

try {
  let releaseExists = false;

  try {
    execFileSync("gh", ["release", "view", releaseTag], {
      cwd: rootDir,
      stdio: "ignore",
    });
    releaseExists = true;
  } catch (_error) {
    releaseExists = false;
  }

  const ghArgs = releaseExists
    ? [
        "release",
        "edit",
        releaseTag,
        "--title",
        releaseTag,
        "--notes-file",
        notesPath,
      ]
    : [
        "release",
        "create",
        releaseTag,
        "--title",
        releaseTag,
        "--notes-file",
        notesPath,
      ];

  execFileSync("gh", ghArgs, {
    cwd: rootDir,
    stdio: "inherit",
  });

  const releaseAssets = findReleaseAssets(releasedVersion);
  if (releaseAssets.length === 0) {
    throw new Error(
      `Failed to find DMG asset for version ${releasedVersion} in ${dmgOutputDir}`
    );
  }

  console.log("Uploading release assets:");
  for (const assetPath of releaseAssets) {
    console.log(`- ${path.relative(rootDir, assetPath)}`);
  }

  execFileSync(
    "gh",
    ["release", "upload", releaseTag, ...releaseAssets, "--clobber"],
    {
      cwd: rootDir,
      stdio: "inherit",
    }
  );
} finally {
  fs.rmSync(notesPath, { force: true });
}
