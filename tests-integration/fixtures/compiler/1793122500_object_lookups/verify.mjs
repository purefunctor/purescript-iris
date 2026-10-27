import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const object = { a: 1, "b-c": 2 };
strictEqual(Main.member("a")(object), true);
strictEqual(Main.member("z")(object), false);
strictEqual(Main.member("constructor")({}), true);
deepStrictEqual(Main.keys(object), ["a", "b-c"]);
strictEqual(Main.index("b-c")(object), 2);
strictEqual(Main.indexLiteral(object), 3);
