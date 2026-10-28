import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.countdown(4)(), 13);
strictEqual(Main.countdown(0)(), 3);
