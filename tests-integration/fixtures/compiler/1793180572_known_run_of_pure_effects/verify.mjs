import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.performPure(1), 2);
strictEqual(Main.runPure(3), 6);
strictEqual(Main.runReference(4), 4);
