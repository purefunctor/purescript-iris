import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.statementIf(true)(), 11);
strictEqual(Main.statementIf(false)(), 12);

strictEqual(Main.boundIf(true)(), 6);
strictEqual(Main.boundIf(false)(), 1);

let performed = 0;
let produced = 0;
const perform = () => {
  performed += 1;
};
const produce = () => {
  produced += 1;
  return 41;
};
strictEqual(Main.opaqueBranches(true)(perform)(produce)(), 42);
strictEqual(performed, 1);
strictEqual(produced, 1);
strictEqual(Main.opaqueBranches(false)(perform)(produce)(), 2);
strictEqual(performed, 1);
strictEqual(produced, 1);
