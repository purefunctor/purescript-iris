import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.loopBody(5)(), 503);
strictEqual(Main.loopBody(0)(), 0);

strictEqual(Main.nestedConditionals(20)(), 22);
strictEqual(Main.nestedConditionals(5)(), 11);
strictEqual(Main.nestedConditionals(0)(), 0);

strictEqual(Main.tailConditional(4), 8);
strictEqual(Main.tailConditional(-4), -4);
