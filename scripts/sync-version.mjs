#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const checkOnly = args.includes("--check");

if (args.length > (checkOnly ? 1 : 0)) {
  console.error("Usage: node scripts/sync-version.mjs [--check]");
  process.exit(1);
}

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(scriptDir, "..");

const packageJsonPath = path.join(rootDir, "package.json");
const cargoTomlPath = path.join(rootDir, "src-tauri", "Cargo.toml");
const tauriConfigPath = path.join(rootDir, "src-tauri", "tauri.conf.json");
const readmePath = path.join(rootDir, "README.md");
const readmeZhPath = path.join(rootDir, "README.zh-CN.md");

function readUtf8(filePath) {
  return fs.readFileSync(filePath, "utf8");
}

function writeUtf8(filePath, content) {
  fs.writeFileSync(filePath, content, "utf8");
}

function detectEol(content) {
  return content.includes("\r\n") ? "\r\n" : "\n";
}

function syncCargoTomlVersion(content, version) {
  const eol = detectEol(content);
  const lines = content.split(/\r?\n/);
  let inPackageSection = false;
  let packageSectionSeen = false;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];

    if (/^\s*\[package\]\s*$/.test(line)) {
      inPackageSection = true;
      packageSectionSeen = true;
      continue;
    }

    if (inPackageSection && /^\s*\[.+\]\s*$/.test(line)) {
      inPackageSection = false;
    }

    if (inPackageSection && /^\s*version\s*=/.test(line)) {
      const nextLine = `version = "${version}"`;
      const changed = line !== nextLine;
      if (changed) {
        lines[i] = nextLine;
      }
      return { changed, content: lines.join(eol) };
    }
  }

  if (!packageSectionSeen) {
    throw new Error("Failed to find [package] section in src-tauri/Cargo.toml");
  }
  throw new Error("Failed to find package version in src-tauri/Cargo.toml");
}

function syncTauriConfigVersion(content, version) {
  const eol = detectEol(content);
  const parsed = JSON.parse(content);
  const changed = parsed.version !== version;
  if (changed) {
    parsed.version = version;
  }
  return { changed, content: `${JSON.stringify(parsed, null, 2)}${eol}` };
}

function syncReadmeBadge(content, version, fileLabel) {
  const versionBadgePattern =
    /(https:\/\/img\.shields\.io\/badge\/version-)([^"]+?)(-blue)/g;

  let found = false;
  const nextContent = content.replace(
    versionBadgePattern,
    (_full, prefix, _currentVersion, suffix) => {
      found = true;
      return `${prefix}${version}${suffix}`;
    }
  );

  if (!found) {
    throw new Error(`Failed to find version badge in ${fileLabel}`);
  }

  return { changed: nextContent !== content, content: nextContent };
}

const packageJson = JSON.parse(readUtf8(packageJsonPath));
const version = packageJson.version;

if (typeof version !== "string" || version.length === 0) {
  throw new Error("package.json version is missing or invalid");
}

const operations = [
  {
    label: "src-tauri/Cargo.toml",
    filePath: cargoTomlPath,
    sync: (content) => syncCargoTomlVersion(content, version),
  },
  {
    label: "src-tauri/tauri.conf.json",
    filePath: tauriConfigPath,
    sync: (content) => syncTauriConfigVersion(content, version),
  },
  {
    label: "README.md",
    filePath: readmePath,
    sync: (content) => syncReadmeBadge(content, version, "README.md"),
  },
  {
    label: "README.zh-CN.md",
    filePath: readmeZhPath,
    sync: (content) => syncReadmeBadge(content, version, "README.zh-CN.md"),
  },
];

const changedLabels = [];

for (const operation of operations) {
  const current = readUtf8(operation.filePath);
  const result = operation.sync(current);

  if (result.changed) {
    changedLabels.push(operation.label);
    if (!checkOnly) {
      writeUtf8(operation.filePath, result.content);
    }
  }
}

if (checkOnly) {
  if (changedLabels.length > 0) {
    console.error(`Version drift detected for ${version}:`);
    for (const label of changedLabels) {
      console.error(`- ${label}`);
    }
    process.exit(1);
  }
  console.log(`Version is synchronized: ${version}`);
} else if (changedLabels.length === 0) {
  console.log(`Version already synchronized: ${version}`);
} else {
  console.log(`Synchronized version ${version} in:`);
  for (const label of changedLabels) {
    console.log(`- ${label}`);
  }
}
