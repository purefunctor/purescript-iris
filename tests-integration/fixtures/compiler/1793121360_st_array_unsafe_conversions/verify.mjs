import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.literal, [1, 2, 3]);

const values = [1];
const shared = Main.shared(values);
strictEqual(shared, values);
deepStrictEqual(values, [1, 0]);
