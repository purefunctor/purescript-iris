import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.results(), {
  written: undefined,
  modified: 12,
  modifiedUnit: undefined,
  modifiedValue: "24",
  final: 25,
});

const ref = { value: 0 };
const increment = Main.increment(ref);
strictEqual(ref.value, 0);
strictEqual(increment(), undefined);
increment();
strictEqual(ref.value, 2);
