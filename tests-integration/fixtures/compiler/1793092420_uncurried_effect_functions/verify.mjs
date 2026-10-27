import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const ref = { value: 10 };
strictEqual(Main.addTo(ref, 5), 15);
strictEqual(ref.value, 15);

deepStrictEqual(Main.called(), { added: 3, external: 7 });
strictEqual(Main.partiallyApplied(41)(), 42);
strictEqual(Main.multiplied, 42);
