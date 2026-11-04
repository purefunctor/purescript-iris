import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
deepStrictEqual(reports, [
  "release fails: failed release",
  "body fails: failed body",
  "both fail: failed body",
  "neither fails: succeeded 2",
]);
