import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

resetTrace();
strictEqual(Main.indexBeforeCase(["a", "b", "c"])(0), "a");
deepStrictEqual(readTrace(), ["array", "scrutinee", "first"]);

resetTrace();
strictEqual(Main.indexBeforeCase(["a", "b", "c"])(1), "c");
deepStrictEqual(readTrace(), ["array", "scrutinee", "last"]);
