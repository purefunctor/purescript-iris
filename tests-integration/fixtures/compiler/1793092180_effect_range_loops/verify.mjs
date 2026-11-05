import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

resetTrace();
strictEqual(Main.sumRange(2)(6)(), 2 + 3 + 4 + 5);
deepStrictEqual(readTrace(), ["start", "end"]);

strictEqual(Main.sumRange(5)(2)(), 0);
strictEqual(Main.sumRange(3)(3)(), 0);
strictEqual(Main.sumWithFunction(5)(), 0 + 1 + 2 + 3 + 4);
strictEqual(Main.productRange(6), 1 * 2 * 3 * 4 * 5);
