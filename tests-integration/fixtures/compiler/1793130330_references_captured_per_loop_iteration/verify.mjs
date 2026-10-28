import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.sharedCounter(), [1, 2, 3]);
deepStrictEqual(Main.sharedCounter(), [1, 2, 3]);
deepStrictEqual(Main.ownCounters(), [2, 12, 22]);
