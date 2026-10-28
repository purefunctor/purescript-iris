import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.inserted(3), { x: 1, y: 2, z: 3 });
deepStrictEqual(Main.reshaped(10), { x: 11, w: 3, z: "new" });
deepStrictEqual(Main.composedBackwards, { x: 1, y: 40 });
deepStrictEqual(Main.fromScratch(1), { x: 1, y: 2 });
deepStrictEqual(Main.merged, { x: 1, y: 20, z: 30 });
