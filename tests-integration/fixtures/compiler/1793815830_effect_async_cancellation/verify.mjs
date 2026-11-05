import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
await new Promise((resolve) => setTimeout(resolve, 100));
deepStrictEqual(reports, [
  "forked",
  "canceler",
  "killed",
  "sleeping failed stop",
  "finished succeeded",
  "done",
]);
