import { deepStrictEqual, strictEqual, throws } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.statementCase(Main.First)(), 11);
strictEqual(Main.statementCase(Main.Second(5))(), 15);
strictEqual(Main.statementCase(Main.Third)(), 10);
deepStrictEqual(Main.takeLog(), ["observe 1", "observe 2", "observe 3"]);

strictEqual(Main.guardedCase(Main.Second(3))(), 3);
deepStrictEqual(Main.takeLog(), ["before", "after"]);
strictEqual(Main.guardedCase(Main.Second(-3))(), 0);
deepStrictEqual(Main.takeLog(), ["before", "negative", "after"]);
strictEqual(Main.guardedCase(Main.Third)(), -1);
deepStrictEqual(Main.takeLog(), ["before", "after"]);

strictEqual(Main.partialCase(Main.First)(), undefined);
deepStrictEqual(Main.takeLog(), ["before", "first", "after"]);
const failing = Main.partialCase(Main.Third);
deepStrictEqual(Main.takeLog(), []);
throws(failing, { message: "Pattern match failure" });
deepStrictEqual(Main.takeLog(), ["before"]);
