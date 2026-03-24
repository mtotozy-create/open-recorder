#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
export const rootDir = path.resolve(scriptDir, "..");
const packageJsonPath = path.join(rootDir, "package.json");

const semverTagPattern = /^v?\d+\.\d+\.\d+$/;

const areaDefinitions = [
  {
    id: "frontend-ui",
    label: "Frontend UI",
    matches: (filePath) =>
      filePath === "src/App.tsx" ||
      filePath === "src/styles.css" ||
      filePath.startsWith("src/components/") ||
      filePath.startsWith("src/i18n/"),
  },
  {
    id: "frontend-contracts",
    label: "Frontend Contracts",
    matches: (filePath) =>
      filePath.startsWith("src/lib/") || filePath.startsWith("src/types/"),
  },
  {
    id: "tauri-command-surface",
    label: "Tauri Command Surface",
    matches: (filePath) =>
      filePath === "src-tauri/src/lib.rs" ||
      filePath.startsWith("src-tauri/src/commands/"),
  },
  {
    id: "tauri-core-services",
    label: "Tauri Core Services",
    matches: (filePath) =>
      filePath === "src-tauri/src/models.rs" ||
      filePath === "src-tauri/src/state.rs" ||
      filePath === "src-tauri/src/storage.rs" ||
      filePath.startsWith("src-tauri/src/providers/"),
  },
  {
    id: "python-local-stt",
    label: "Python Local STT",
    matches: (filePath) => filePath.startsWith("src-tauri/python/"),
  },
];

const publicContractMatchers = [
  (filePath) => filePath === "src/lib/api.ts",
  (filePath) => filePath.startsWith("src/types/"),
  (filePath) => filePath === "src-tauri/src/lib.rs",
  (filePath) => filePath.startsWith("src-tauri/src/commands/"),
];

export function run(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: rootDir,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  }).trim();
}

export function bumpSemver(version, bumpType) {
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

export function getCurrentVersion() {
  const pkg = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  const { version } = pkg;

  if (typeof version !== "string" || version.length === 0) {
    throw new Error("package.json version is missing");
  }

  return version;
}

export function getLatestSemverTag() {
  const tagOutput = run("git", ["tag", "--merged", "HEAD", "--sort=-v:refname"]);
  const tags = tagOutput
    .split("\n")
    .map((tag) => tag.trim())
    .filter(Boolean);

  return tags.find((tag) => semverTagPattern.test(tag)) ?? null;
}

function getCommitRange(baseTag) {
  return baseTag ? `${baseTag}..HEAD` : null;
}

function getChangedFiles(baseTag) {
  if (baseTag) {
    const diffOutput = run("git", ["diff", "--name-only", `${baseTag}..HEAD`]);
    if (!diffOutput) {
      return [];
    }

    return diffOutput.split("\n").map((filePath) => filePath.trim()).filter(Boolean);
  }

  const treeOutput = run("git", ["ls-tree", "-r", "--name-only", "HEAD"]);
  if (!treeOutput) {
    return [];
  }

  return treeOutput.split("\n").map((filePath) => filePath.trim()).filter(Boolean);
}

function getCommits(baseTag) {
  const args = ["log", "--format=%H%x1f%s%x1f%b%x1e"];
  const range = getCommitRange(baseTag);
  if (range) {
    args.push(range);
  }

  const output = run("git", args);
  if (!output) {
    return [];
  }

  return output
    .split("\x1e")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [hash = "", subject = "", body = ""] = entry.split("\x1f");
      return {
        hash: hash.trim(),
        subject: subject.trim(),
        body: body.trim(),
      };
    });
}

function classifyChangedAreas(changedFiles) {
  const matchedAreas = [];

  for (const area of areaDefinitions) {
    if (changedFiles.some((filePath) => area.matches(filePath))) {
      matchedAreas.push({
        id: area.id,
        label: area.label,
      });
    }
  }

  return matchedAreas;
}

function isBreakingCommit(commit) {
  return (
    /^[a-z]+(?:\([^)]+\))?!:/i.test(commit.subject) ||
    /BREAKING CHANGE:/i.test(commit.body)
  );
}

function isFeatureCommit(commit) {
  return /^feat(?:\([^)]+\))?:/i.test(commit.subject);
}

function hasPublicContractChange(changedFiles) {
  return changedFiles.some((filePath) =>
    publicContractMatchers.some((matches) => matches(filePath))
  );
}

export function getReleaseRecommendation() {
  const currentVersion = getCurrentVersion();
  const baseTag = getLatestSemverTag();
  const changedFiles = getChangedFiles(baseTag);
  const commits = getCommits(baseTag);
  const changedAreas = classifyChangedAreas(changedFiles);
  const breakingCommits = commits.filter(isBreakingCommit);
  const featureCommits = commits.filter(isFeatureCommit);
  const publicContractChanged = hasPublicContractChange(changedFiles);
  const hasUserVisibleFeatureSignal =
    featureCommits.length > 0 || publicContractChanged;

  let recommendedBump = "patch";
  const reasonLines = [];

  if (breakingCommits.length > 0) {
    recommendedBump = "major";
    reasonLines.push(
      `Detected breaking change commit: ${breakingCommits[0].subject}`
    );
  } else if (changedAreas.length >= 3 && hasUserVisibleFeatureSignal) {
    recommendedBump = "minor";
    reasonLines.push(
      `Detected user-visible feature signal: ${
        featureCommits.length > 0 ? "feat commit(s)" : "public contract change"
      }`
    );
    reasonLines.push(
      `Changed ${changedAreas.length} core areas (threshold: 3): ${changedAreas
        .map((area) => area.label)
        .join(", ")}`
    );
  } else {
    if (!hasUserVisibleFeatureSignal) {
      reasonLines.push("No user-visible feature signal detected.");
    } else {
      reasonLines.push(
        `User-visible feature signal detected, but only ${changedAreas.length} core area(s) changed.`
      );
    }

    if (changedAreas.length > 0) {
      reasonLines.push(
        `Changed core areas: ${changedAreas.map((area) => area.label).join(", ")}`
      );
    } else {
      reasonLines.push("No core release areas changed.");
    }
  }

  return {
    baseTag,
    currentVersion,
    changedAreas,
    changedFiles,
    commits,
    featureCommits,
    hasUserVisibleFeatureSignal,
    nextVersion: bumpSemver(currentVersion, recommendedBump),
    publicContractChanged,
    reasonLines,
    recommendedBump,
  };
}

export function getChangelogPreview() {
  return run("conventional-changelog", [
    "-p",
    "conventionalcommits",
    "-t",
    "v",
    "-r",
    "1",
    "--stdout",
  ]);
}
