import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const lines = [];
run((line) => () => lines.push(line))();
deepStrictEqual(lines, ["12 first", "true"]);
