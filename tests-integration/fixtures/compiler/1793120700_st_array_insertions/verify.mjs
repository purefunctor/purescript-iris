import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.inserted([5, 6]), [5, 6, 1, 2, 3, 4, 5, 6, 9]);
deepStrictEqual(Main.lengths, [1, 3, 4, 6]);
