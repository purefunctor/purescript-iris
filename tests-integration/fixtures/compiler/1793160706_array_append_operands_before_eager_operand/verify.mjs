import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

resetTrace();
deepStrictEqual(Main.appendBeforeCase([1, 2])(0), [0, 1, 2]);
deepStrictEqual(readTrace(), ["literal", "spread", "scrutinee"]);

resetTrace();
deepStrictEqual(Main.appendBeforeCase([1, 2])(3), [0, 1, 2, 3]);
deepStrictEqual(readTrace(), ["literal", "spread", "scrutinee", "nonempty"]);
