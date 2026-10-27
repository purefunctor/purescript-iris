import { deepStrictEqual } from "node:assert/strict";
import { Just } from "./output/Data.Maybe/index.js";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

resetTrace();
deepStrictEqual(Main.accesses([1, 2, 3]), {
  peeked: Just(2),
  poked: false,
  final: [1, 2, 3],
});
deepStrictEqual(readTrace(), [
  "peek index",
  "peek array",
  "poke index",
  "poke value",
  "poke array",
]);
