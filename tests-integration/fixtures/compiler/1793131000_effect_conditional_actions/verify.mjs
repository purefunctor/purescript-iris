import { deepStrictEqual, strictEqual, throws } from "node:assert/strict";
import { diagonal, genericWhen, unlessConfigured, whenConfigured } from "./output/Main/index.js";

for (const [program, enabled] of [[whenConfigured, true], [unlessConfigured, false]]) {
  for (const condition of [false, true]) {
    const events = [];
    const reference = { value: 0 };
    const action = program({
      get enabled() { events.push("condition"); return condition; },
      get value() { events.push("value"); return 7; },
    })(reference);
    deepStrictEqual(events, ["condition", "value"]);
    strictEqual(reference.value, 0);
    action();
    strictEqual(reference.value, condition === enabled ? 7 : 0);
    reference.value = 2;
    action();
    strictEqual(reference.value, condition === enabled ? 7 : 2);
    deepStrictEqual(events, ["condition", "value"]);
  }
  throws(() => program({
    enabled: !enabled,
    get value() { throw new Error("construction"); },
  })({ value: 0 }), /construction/);
}

strictEqual(diagonal(0)(), 0);
strictEqual(diagonal(7)(), 7);
const dictionary = { pure: value => ({ pure: value }) };
const action = { custom: true };
strictEqual(genericWhen(dictionary)(true)(action), action);
deepStrictEqual(genericWhen(dictionary)(false)(action), { pure: undefined });
