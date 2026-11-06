import { deepStrictEqual, strictEqual, throws } from "node:assert/strict";
import { failWhen, run, unlessConfigured, whenConfigured } from "./output/Main/index.js";

const execute = program => new Promise(resolve => run(program)(exit => () => resolve(exit))());

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
    deepStrictEqual(await execute(action), { tag: "Succeeded", _1: undefined });
    strictEqual(reference.value, condition === enabled ? 7 : 0);
    reference.value = 2;
    deepStrictEqual(await execute(action), { tag: "Succeeded", _1: undefined });
    strictEqual(reference.value, condition === enabled ? 7 : 2);
    deepStrictEqual(events, ["condition", "value"]);
  }
  throws(() => program({
    enabled: !enabled,
    get value() { throw new Error("construction"); },
  })({ value: 0 }), /construction/);
}

deepStrictEqual(await execute(failWhen(false)), { tag: "Succeeded", _1: undefined });
deepStrictEqual(await execute(failWhen(true)), { tag: "Failed", _1: "failure" });
