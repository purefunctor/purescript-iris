import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const reports = [];
run((message) => () => reports.push(message))();
deepStrictEqual(reports, ["nested forks 719", "binds 100000"]);
