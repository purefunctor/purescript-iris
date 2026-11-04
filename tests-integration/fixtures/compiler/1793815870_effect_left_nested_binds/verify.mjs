import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
await new Promise((resolve) => setTimeout(resolve, 100));
deepStrictEqual(reports, [
  "total 100000",
  "effects 100000 100000",
  "mapped 100000",
  "applied 100000",
]);
