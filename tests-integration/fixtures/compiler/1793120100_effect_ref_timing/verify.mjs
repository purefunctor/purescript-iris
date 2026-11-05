import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { marks, poke } from "./output/Main/foreign.js";

const reference = Main.fresh();
const writeArgument = Main.writeArgument(reference);
deepStrictEqual(marks.splice(0), ["write argument"]);
strictEqual(writeArgument(), 6);
deepStrictEqual(marks.splice(0), ["modify function"]);
strictEqual(writeArgument(), 6);
deepStrictEqual(marks.splice(0), ["modify function"]);

const readLater = Main.readLater(reference);
poke(reference)(9);
strictEqual(readLater(), 9);
poke(reference)(10);
strictEqual(readLater(), 10);

deepStrictEqual(Main.reusedRead(reference)(), { before: 10, after: 7 });

poke(reference)(3);
deepStrictEqual(Main.modifyOrder(reference)(), { returned: 4, seen: 3, state: 30 });

poke(reference)(1);
deepStrictEqual(Main.modifyResults(reference)(), { modified: 2, discarded: undefined, final: 4 });

strictEqual(Main.newPerExecution(), 1);
strictEqual(Main.newPerExecution(), 1);
