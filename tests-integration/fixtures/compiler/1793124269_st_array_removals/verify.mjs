import { deepStrictEqual } from "node:assert/strict";
import { Just, Nothing } from "./output/Data.Maybe/index.js";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.removals([1, 2, 3]), {
  last: Just(3),
  first: Just(1),
  emptyLast: Nothing,
  emptyFirst: Nothing,
  final: [2],
});
deepStrictEqual(Main.removals([undefined]), {
  last: Just(undefined),
  first: Nothing,
  emptyLast: Nothing,
  emptyFirst: Nothing,
  final: [],
});
