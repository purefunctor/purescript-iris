import { strictEqual } from "node:assert/strict";
import * as Effect_Ref from "./output/Effect.Ref/index.js";
import * as Main from "./output/Main/index.js";

const readBeforeWrite = Main.readBeforeWrite(1);
strictEqual(readBeforeWrite(), 15);
strictEqual(readBeforeWrite(), 15);
strictEqual(Main.sumTwo(1)(10)(), 12);

const escapes = Main.escapes(2);
const escaped = escapes();
const another = escapes();
strictEqual(Effect_Ref.read(escaped)(), 4);
Effect_Ref.write(9)(escaped)();
strictEqual(Effect_Ref.read(another)(), 4);

strictEqual(Main.passedToFunction(2)(), 3);
