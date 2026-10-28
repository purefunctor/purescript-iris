import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.performedInside(), 4);
strictEqual(Main.performedInside(), 4);
strictEqual(Main.runInside(1), 4);
strictEqual(Main.runInside(5), 12);
