import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import * as Unsafe from "./output/Record.Unsafe/index.js";

deepStrictEqual(Main.present, Unsafe.unsafeHas("x")({ x: 1, y: "y" }));
deepStrictEqual(Main.absent, Unsafe.unsafeHas("z")({ x: 1, y: "y" }));
deepStrictEqual(Main.unknown({ x: 1 }), true);

Main.readTrace(true);
deepStrictEqual(Main.presentOrder(false), true);
deepStrictEqual(Main.readTrace(true), ["x", "y"]);
