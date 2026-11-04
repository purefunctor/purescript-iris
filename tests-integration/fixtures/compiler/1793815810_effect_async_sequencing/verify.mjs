import { deepStrictEqual } from "node:assert/strict";
import { runOrdered, provided } from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

deepStrictEqual(readTrace(), []);
const action = runOrdered();
deepStrictEqual(readTrace(), ["environment", "program"]);
for (let iteration = 0; iteration < 2; iteration++) {
  resetTrace();
  action();
  deepStrictEqual(readTrace(), [
    "construct:first:11", "run:first",
    "construct:left:60", "construct:right:7", "run:left", "run:right", "result:53",
  ]);
}
resetTrace();
provided();
deepStrictEqual(readTrace(), [
  "construct:first:5", "run:first", "construct:left:30", "construct:right:3",
  "run:left", "run:right", "result:27",
]);
