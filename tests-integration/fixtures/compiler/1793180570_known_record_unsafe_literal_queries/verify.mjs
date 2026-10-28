import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.literalHasX, true);
strictEqual(Main.literalHasZ, false);
strictEqual(Main.hasX({ x: 1, y: 2 }), true);
strictEqual(Main.hasX(Object.create({ x: 1 })), false);
deepStrictEqual(Main.literalDeleteY(1), { x: 1 });

const point = { x: 1, y: 2 };
deepStrictEqual(Main.deleteY(point), { x: 1 });
deepStrictEqual(point, { x: 1, y: 2 });
