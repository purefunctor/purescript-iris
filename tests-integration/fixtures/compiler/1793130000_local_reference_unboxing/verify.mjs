import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.refs(), 2);
strictEqual(Main.refs(), 2);
strictEqual(Main.branches(true)(), "then!");
strictEqual(Main.branches(false)(), "start-else");
deepStrictEqual(Main.stateWrites(3), [6, 7, 7]);
