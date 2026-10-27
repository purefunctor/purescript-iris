import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.copies([1, 2]), {
  original: [1, 2],
  frozen: [1, 2, 3],
  cloned: [1, 2, 3, 5],
  final: [1, 2, 3, 4],
});
