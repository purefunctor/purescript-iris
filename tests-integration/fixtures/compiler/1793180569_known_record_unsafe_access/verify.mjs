import { deepStrictEqual, notStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.readX({ x: 1, y: 2 }), 1);
strictEqual(Main.readSpacedLabel({ "first name": "Ada" }), "Ada");
strictEqual(Main.readLabel("y")({ x: 1, y: 2 }), 2);

const point = { x: 1, y: 2 };
const moved = Main.setX(10)(point);
deepStrictEqual(moved, { x: 10, y: 2 });
notStrictEqual(moved, point);
deepStrictEqual(point, { x: 1, y: 2 });
deepStrictEqual(Main.addZ(3)(point), { x: 1, y: 2, z: 3 });
deepStrictEqual(point, { x: 1, y: 2 });
