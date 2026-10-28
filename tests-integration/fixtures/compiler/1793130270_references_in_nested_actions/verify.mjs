import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.captured(true)(), 12);
strictEqual(Main.captured(false)(), 2);

const bump = Main.capturedAction();
strictEqual(bump(), 2);
strictEqual(bump(), 3);
strictEqual(Main.capturedAction()(), 2);

const { increment, current } = Main.counter();
strictEqual(current(), 1);
increment();
increment();
strictEqual(current(), 3);
strictEqual(Main.counter().current(), 1);
