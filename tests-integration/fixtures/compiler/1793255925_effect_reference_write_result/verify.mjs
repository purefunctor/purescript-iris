import { strictEqual } from "node:assert/strict";
import * as Effect_Ref from "./output/Effect.Ref/index.js";
import * as Main from "./output/Main/index.js";

const reference = Effect_Ref.new(1)();
strictEqual(Main.writeResult(reference)(), 7);
strictEqual(Effect_Ref.read(reference)(), 7);
strictEqual(Main.boundWriteResult(reference)(), 8);
strictEqual(Effect_Ref.read(reference)(), 8);
strictEqual(Main.localWriteResult(2)(), 3);
