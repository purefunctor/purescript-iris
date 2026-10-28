import * as Data_Boolean from "../Data.Boolean/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as $foreign from "./foreign.js";
export const First = "First";
export const Second = ($value0) => ({
  tag: "Second",
  _1: $value0
});
export const Third = "Third";
export function statementCase(choice) {
  return () => {
    let ref = 0 | 0;
    const $scrutinee = observe(choice);
    $case: {
      if ($scrutinee === "First") {
        ref = 1 | 0;
        break $case;
      }
      if ($scrutinee.tag === "Second") {
        const { _1: count } = $scrutinee;
        ref = count;
        break $case;
      }
      if ($scrutinee === "Third") {
        break $case;
      }
      throw new Error("Pattern match failure");
    }
    ref = ref + (10 | 0) | 0;
    return ref;
  };
}
export function guardedCase(choice) {
  const $action = record("before");
  return () => {
    const $unit = $action();
    let value;
    $case: {
      if (choice.tag === "Second") {
        const { _1: count } = choice;
        if (count > (0 | 0)) {
          value = count;
          break $case;
        }
        if (Data_Boolean.otherwise) {
          const $action$1 = record("negative");
          const $unit$1 = $action$1();
          value = 0 | 0;
          break $case;
        }
      }
      value = -1 | 0;
      break $case;
    }
    const $action$2 = record("after");
    const $unit$2 = $action$2();
    return value;
  };
}
export function partialCase(choice) {
  const $action = record("before");
  return () => {
    const $unit = $action();
    const $closure = (partialDict) => {
      if (choice === "First") {
        return record("first");
      }
      throw new Error("Pattern match failure");
    };
    const $action$1 = /* @__PURE__ */ $closure({});
    const $unit$1 = $action$1();
    return record("after")();
  };
}
export const observe = $foreign["observe"];
export const record = $foreign["record"];
export const takeLog = $foreign["takeLog"];
