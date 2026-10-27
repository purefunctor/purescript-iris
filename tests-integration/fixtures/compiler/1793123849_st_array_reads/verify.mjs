import { deepStrictEqual } from "node:assert/strict";
import { Just, Nothing } from "./output/Data.Maybe/index.js";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.reads([4, 5, 6]), {
  length: 3,
  first: Just(4),
  last: Just(6),
  past: Nothing,
  negative: Nothing,
});
deepStrictEqual(Main.reads([]), {
  length: 0,
  first: Nothing,
  last: Nothing,
  past: Nothing,
  negative: Nothing,
});
