import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.performed, 21);
strictEqual(Main.doubled(21), 42);
strictEqual(Main.offset(41), 42);

let runs = 0;
const action = () => {
  runs += 1;
  return 42;
};
strictEqual(Main.performedAction(action), 42);
strictEqual(Main.runAction(action), 42);
strictEqual(runs, 2);
