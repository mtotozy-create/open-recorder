#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import process from "node:process";
import { getReleaseRecommendation, rootDir } from "./release-recommendation.mjs";

const passthroughArgs = process.argv.slice(2);
const recommendation = getReleaseRecommendation();

console.log("Auto Release");
console.log(`Base tag: ${recommendation.baseTag ?? "(none)"}`);
console.log(`Recommended bump: ${recommendation.recommendedBump}`);
console.log(`Next version: ${recommendation.nextVersion}`);

for (const reason of recommendation.reasonLines) {
  console.log(`- ${reason}`);
}

execFileSync(
  "release-it",
  [recommendation.recommendedBump, ...passthroughArgs],
  {
    cwd: rootDir,
    stdio: "inherit",
  }
);
