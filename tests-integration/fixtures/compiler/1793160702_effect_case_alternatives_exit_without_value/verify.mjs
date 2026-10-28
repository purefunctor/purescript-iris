import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const describe = (render, value) => {
  const log = { value: [] };
  strictEqual(render({ value })(log)(), undefined);
  return log.value;
};

deepStrictEqual(describe(Main.describe, 0), ["zero"]);
deepStrictEqual(describe(Main.describe, 1), ["one"]);
deepStrictEqual(describe(Main.describe, 2), ["many"]);

deepStrictEqual(describe(Main.describeGuarded, 11), ["big"]);
deepStrictEqual(describe(Main.describeGuarded, 3), ["small"]);
deepStrictEqual(describe(Main.describeGuarded, 0), ["none"]);

deepStrictEqual(Main.describeEach([0, 1, 2, 0])(), ["zero", "one", "many", "zero"]);
deepStrictEqual(Main.describeCountdown(2)(), ["many", "one", "zero"]);
