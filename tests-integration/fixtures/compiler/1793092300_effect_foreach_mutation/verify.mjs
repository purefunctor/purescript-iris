import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const values = [1, 2, 3];
deepStrictEqual(Main.visit(values)(), [1, 2, 99]);
deepStrictEqual(values, [1, 2, 99, 1, 2, 99]);

deepStrictEqual(Main.visitWithFunction([4, 5])(), [4, 5]);
deepStrictEqual(Main.visitWithFunction([])(), []);
strictEqual(Main.sumState([1, 2, 3, 4]), 10);
