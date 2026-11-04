import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
deepStrictEqual(reports, [
  "before fail",
  "caught boom",
  "recovered 7",
  "left mapped inner",
  "left handled first",
  "right 3",
  "uncaught 42",
]);
