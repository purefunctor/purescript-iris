import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
deepStrictEqual(reports, [
  "built",
  "start",
  "hello outer, hello inner, hello outer & 2",
  "start",
  "hello outer, hello inner, hello outer & 2",
]);
