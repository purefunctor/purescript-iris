import { deepStrictEqual } from "node:assert/strict";
import { results } from "./output/Main/index.js";

deepStrictEqual(results, [true, true, true, true, true, true, true]);
