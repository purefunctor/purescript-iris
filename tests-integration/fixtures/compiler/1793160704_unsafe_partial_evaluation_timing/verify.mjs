import { deepStrictEqual, strictEqual, throws } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

deepStrictEqual(readTrace(), ["function"]);

resetTrace();
strictEqual(Main.tracedValue(1), 1);
deepStrictEqual(readTrace(), ["inside", "after argument"]);

resetTrace();
throws(() => Main.tracedCrash(2), { message: "crashed" });
deepStrictEqual(readTrace(), ["message"]);

resetTrace();
strictEqual(Main.tracedFunction(3), 3);
strictEqual(Main.tracedFunction(4), 4);
deepStrictEqual(readTrace(), ["body", "body"]);
