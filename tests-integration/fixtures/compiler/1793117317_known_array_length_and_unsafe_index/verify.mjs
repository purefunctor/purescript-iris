import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.arrayLength([]), 0);
strictEqual(Main.arrayLength(["a", "b", "c"]), 3);
strictEqual(Main.firstValue(["a", "b", "c"]), "a");
strictEqual(Main.lastValue(["a", "b", "c"]), "c");
strictEqual(Main.firstValue([]), undefined);
strictEqual(Main.lastValue([]), undefined);
strictEqual(Main.indexWith({})(["a", "b"])(1), "b");
