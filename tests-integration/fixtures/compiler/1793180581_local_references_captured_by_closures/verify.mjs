import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.delayedRead(1)(), 7);

strictEqual(Main.closureWrites(), 2);
strictEqual(Main.closureWrites(), 2);

const countDown = Main.countDown(3);
strictEqual(countDown(), 3);
strictEqual(countDown(), 3);
strictEqual(Main.countDown(0)(), 0);
