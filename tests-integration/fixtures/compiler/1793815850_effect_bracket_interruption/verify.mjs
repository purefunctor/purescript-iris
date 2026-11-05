import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
await new Promise((resolve) => setTimeout(resolve, 200));
deepStrictEqual(reports, [
  "acquired",
  "released",
  "during acquire failed kill during acquire",
  "acquired",
  "cancel use",
  "released",
  "during use failed kill during use",
  "cancel use",
  "with failing release failed kill with failing release",
]);
