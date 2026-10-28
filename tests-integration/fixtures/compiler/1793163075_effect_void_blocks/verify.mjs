import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.finalRead(), 3);

strictEqual(Main.finalPure(5)(), 12);
deepStrictEqual(Main.takeLog(), ["observe 5"]);

strictEqual(Main.finalMap(), 4);
deepStrictEqual(Main.takeLog(), ["observe 4"]);
