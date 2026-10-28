import { strictEqual } from "node:assert/strict";
import * as Ref from "./output/Effect.Ref/index.js";
import * as Main from "./output/Main/index.js";

strictEqual(Main.passed(), 11);
strictEqual(Ref.read(Main.returned())(), 2);

const { counter } = Main.stored();
strictEqual(Ref.read(counter)(), 6);
Ref.write(7)(counter)();
strictEqual(Ref.read(counter)(), 7);

const escaped = Main.escapesFromClosure()(undefined);
strictEqual(Ref.read(escaped)(), 1);
Ref.write(5)(escaped)();
strictEqual(Ref.read(escaped)(), 5);

const later = Main.passedLater();
strictEqual(later(), 1);
strictEqual(later(), 2);
