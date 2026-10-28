import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.literalUnion(1), { x: 1, y: 2, z: 30 });
deepStrictEqual(Main.literalUnionFn(2), { x: 1, y: 2, z: 30 });
deepStrictEqual(Main.unionWithLiteral({ y: 20, z: 30 }), { x: 1, y: 2, z: 30 });
