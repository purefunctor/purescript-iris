import { strictEqual, throws } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.unusedDictionary(1), 2);
strictEqual(Main.forwardedDictionary(3), 3);
throws(() => Main.forwardedDictionary(0), { message: "Pattern match failure" });
throws(() => Main.forwardedCrash(7), { message: "crashed with 7" });
strictEqual(Main.extraArguments(5), 15);
