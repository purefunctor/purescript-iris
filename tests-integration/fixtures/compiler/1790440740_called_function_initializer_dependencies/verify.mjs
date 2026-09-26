import { strictEqual } from "node:assert/strict";
import { conditional, test } from "./output/Main/index.js";

strictEqual(test, 7);
strictEqual(conditional, 19);
