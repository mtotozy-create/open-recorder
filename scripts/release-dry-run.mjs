#!/usr/bin/env node

import {
  getChangelogPreview,
  getReleaseRecommendation,
} from "./release-recommendation.mjs";

const recommendation = getReleaseRecommendation();
const changelogPreview = getChangelogPreview();

console.log("Release Preview");
console.log(`Current version: ${recommendation.currentVersion}`);
console.log(`Base tag: ${recommendation.baseTag ?? "(none)"}`);
console.log(`Recommended bump: ${recommendation.recommendedBump}`);
console.log(`Next version: ${recommendation.nextVersion}`);
console.log(
  `Changed core areas: ${
    recommendation.changedAreas.length > 0
      ? recommendation.changedAreas.map((area) => area.label).join(", ")
      : "(none)"
  }`
);
console.log(
  `Feature signal: ${
    recommendation.hasUserVisibleFeatureSignal ? "yes" : "no"
  }`
);
console.log("");
console.log("Recommendation reasons:");
for (const reason of recommendation.reasonLines) {
  console.log(`- ${reason}`);
}
console.log("");
console.log("Changelog preview:");
console.log(changelogPreview);
