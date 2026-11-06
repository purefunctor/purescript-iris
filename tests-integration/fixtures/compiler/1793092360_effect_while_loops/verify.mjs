import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { queue } from "./output/Main/foreign.js";

deepStrictEqual(Main.countdown(3)(), [3, 2, 1]);
deepStrictEqual(Main.countdown(0)(), []);
strictEqual(Main.countUntil(5)(), 5);
strictEqual(Main.countUntil(0)(), 1);

queue.push("a", "b", "c");
strictEqual(Main.drain(), undefined);
strictEqual(queue.length, 0);

strictEqual(Main.doubleUntil(100), 128);
strictEqual(Main.doubleUntil(1), 1);
