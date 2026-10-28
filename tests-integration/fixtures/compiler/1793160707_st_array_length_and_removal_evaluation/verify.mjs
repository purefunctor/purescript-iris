import { deepStrictEqual } from "node:assert/strict";
import { Just, Nothing } from "./output/Data.Maybe/index.js";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

resetTrace();
deepStrictEqual(Main.removals([1, 2, 3]), {
  length: 3,
  last: Just(3),
  first: Just(1),
  final: [2],
});
deepStrictEqual(readTrace(), ["length array", "pop array", "shift array"]);

resetTrace();
deepStrictEqual(Main.removals([]), {
  length: 0,
  last: Nothing,
  first: Nothing,
  final: [],
});
deepStrictEqual(readTrace(), ["length array", "pop array", "shift array"]);
