import { strictEqual, throws } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.unsafePositive(3), 3);
throws(() => Main.unsafePositive(0), { message: "Pattern match failure" });
strictEqual(Main.unsafePositiveFunction(4), 4);
throws(() => Main.unsafePositiveFunction(-1), { message: "Pattern match failure" });
strictEqual(Main.unsafePositiveApplied(5), 5);
strictEqual(Main.unsafeLambda(1), 1);
throws(() => Main.unsafeLambda(2), { message: "Pattern match failure" });
