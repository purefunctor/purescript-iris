import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const call = closures => closures.map(closure => closure(undefined));

deepStrictEqual(call(Main.snapshots()), [10, 20, 30]);
deepStrictEqual(call(Main.previous([4, 5, 6])()), [0, 4, 5]);
strictEqual(Main.chained(), 3);
deepStrictEqual(Main.readers(), [42, 42, 42]);
