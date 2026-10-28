import { strictEqual } from "node:assert/strict";
import * as Ref from "./output/Effect.Ref/index.js";
import * as Main from "./output/Main/index.js";

strictEqual(Main.passed(), 11);
strictEqual(Ref.read(Main.returned())(), 2);

const { counter } = Main.stored();
strictEqual(Ref.read(counter)(), 6);
Ref.write(7)(counter)();
strictEqual(Ref.read(counter)(), 7);

strictEqual(Main.captured(true)(), 12);
strictEqual(Main.captured(false)(), 2);
