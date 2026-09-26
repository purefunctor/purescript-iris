import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.test, -2147483648);
strictEqual(Main.test2, -2147483648);
strictEqual(Main.test3, -2147483648);
strictEqual(Main.test4(-2147483648), true);
strictEqual(Main.test4(-2147483647), false);
strictEqual(Main.test5, -2147483648);
strictEqual(Main.test6, -2147483647);
strictEqual(Main.test7, 2147483647);
strictEqual(Main.test8, 42);
