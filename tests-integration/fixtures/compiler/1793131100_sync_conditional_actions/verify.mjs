import { deepStrictEqual, strictEqual, throws } from "node:assert/strict";
import { run, step, unlessConfigured, whenConfigured } from "./output/Main/index.js";

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
    strictEqual(run(action)(), undefined);
    strictEqual(reference.value, condition === enabled ? 7 : 0);
    reference.value = 2;
    run(action)();
    strictEqual(reference.value, condition === enabled ? 7 : 2);
    deepStrictEqual(events, ["condition", "value"]);
  }
  throws(() => program({
    enabled: !enabled,
    get value() { throw new Error("construction"); },
  })({ value: 0 }), /construction/);
}

const reference = { value: 0 };
const counter = run(step(reference)(2));
deepStrictEqual([counter(), counter(), counter(), counter()], [1, 2, 0, 1]);
