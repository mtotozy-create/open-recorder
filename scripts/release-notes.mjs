#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { rootDir } from "./release-recommendation.mjs";

const changelogPath = path.join(rootDir, "CHANGELOG.md");

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function extractCompareRange(headingLine) {
  const match = headingLine.match(/\/compare\/([^)]+)\)/);
  return match ? match[1] : null;
}

export function getReleaseSection(version) {
  const changelog = fs.readFileSync(changelogPath, "utf8");
  const lines = changelog.split(/\r?\n/);
  const headingPattern = new RegExp(
    `^##\\s+(?:\\[${escapeRegex(version)}\\]\\([^)]*\\)|${escapeRegex(
      version
    )})\\s+\\((\\d{4}-\\d{2}-\\d{2})\\)$`
  );

  let startIndex = -1;
  let releaseDate = null;

  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(headingPattern);
    if (match) {
      startIndex = index;
      releaseDate = match[1];
      break;
    }
  }

  if (startIndex === -1 || !releaseDate) {
    throw new Error(`Failed to find CHANGELOG entry for version ${version}`);
  }

  let endIndex = lines.length;
  for (let index = startIndex + 1; index < lines.length; index += 1) {
    if (/^##\s+/.test(lines[index])) {
      endIndex = index;
      break;
    }
  }

  const headingLine = lines[startIndex];
  const compareRange = extractCompareRange(headingLine);
  const sectionLines = lines.slice(startIndex + 1, endIndex);

  while (sectionLines.length > 0 && sectionLines[0].trim() === "") {
    sectionLines.shift();
  }

  while (
    sectionLines.length > 0 &&
    sectionLines[sectionLines.length - 1].trim() === ""
  ) {
    sectionLines.pop();
  }

  return {
    compareRange,
    releaseDate,
    sectionBody: sectionLines.join("\n").trim(),
    version,
  };
}

export function buildReleaseNotes(version) {
  const section = getReleaseSection(version);
  const lines = [`# Open Recorder v${version}`, ""];

  lines.push(`- Released: ${section.releaseDate}`);
  if (section.compareRange) {
    lines.push(`- Compare: ${section.compareRange}`);
  }

  lines.push("", "## Changelog", "");

  if (section.sectionBody.length > 0) {
    lines.push(section.sectionBody);
  } else {
    lines.push("No changelog details were recorded for this release.");
  }

  lines.push("");

  return lines.join("\n");
}

export function writeReleaseNotesFile(version) {
  const notesPath = path.join(
    os.tmpdir(),
    `open-recorder-release-notes-${version}-${process.pid}.md`
  );

  fs.writeFileSync(notesPath, buildReleaseNotes(version), "utf8");
  return notesPath;
}

function main() {
  const [version, outputPath] = process.argv.slice(2);

  if (!version) {
    console.error(
      "Usage: node scripts/release-notes.mjs <version> [output-path]"
    );
    process.exit(1);
  }

  const content = buildReleaseNotes(version);

  if (outputPath) {
    fs.writeFileSync(outputPath, content, "utf8");
    console.log(outputPath);
    return;
  }

  process.stdout.write(content);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main();
}
