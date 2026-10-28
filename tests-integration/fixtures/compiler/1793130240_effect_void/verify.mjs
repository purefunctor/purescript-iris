import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.statements(), 3);

strictEqual(Main.final(), undefined);
deepStrictEqual(Main.takeLog(), ["first", "second"]);

strictEqual(Main.nested(), undefined);
deepStrictEqual(Main.takeLog(), ["inner", "last"]);

let runs = 0;
const action = () => {
  runs += 1;
  return 42;
};
strictEqual(Main.discarded(action)(), undefined);
strictEqual(runs, 1);

strictEqual(Main.stateWrite, undefined);
