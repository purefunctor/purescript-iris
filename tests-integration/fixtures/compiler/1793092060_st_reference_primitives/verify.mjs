import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.results, {
  written: 2,
  modified: 12,
  modifiedValue: "12",
  final: 24,
});
