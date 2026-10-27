import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.built("b"), { a: 1, b: 2 });
deepStrictEqual(Main.built("removed"), { a: 1 });

const original = { a: 1 };
deepStrictEqual(Main.copies(original), {
  original: { a: 1 },
  frozen: { a: 1, b: 2 },
  final: { b: 2 },
});
