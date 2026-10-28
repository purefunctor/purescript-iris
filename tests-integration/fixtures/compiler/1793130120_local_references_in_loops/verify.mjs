import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.counted([1, 2])(), 25);
strictEqual(Main.counted([30])(), 34);
strictEqual(Main.doubledPerIteration(), 12);
deepStrictEqual(Main.stateCountdown(3), [3, 2, 1]);
deepStrictEqual(Main.stateCountdown(0), []);
