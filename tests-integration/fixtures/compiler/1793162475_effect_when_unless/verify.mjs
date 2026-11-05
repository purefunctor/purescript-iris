import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.statementWhen(true)(), 5);
strictEqual(Main.statementWhen(false)(), 1);

strictEqual(Main.finalWhen(true)(), undefined);
deepStrictEqual(Main.takeLog(), ["before", "inside"]);
strictEqual(Main.finalWhen(false)(), undefined);
deepStrictEqual(Main.takeLog(), ["before"]);

strictEqual(Main.stateUnless(3), 3);
strictEqual(Main.stateUnless(-3), 0);
