import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
await new Promise((resolve) => setTimeout(resolve, 200));
deepStrictEqual(reports, [
  "failed outer 7, failed outer 8",
  "failed joined child failure, succeeded 5, failed killed",
  "caught parent failure",
  "uncaught 99",
]);
