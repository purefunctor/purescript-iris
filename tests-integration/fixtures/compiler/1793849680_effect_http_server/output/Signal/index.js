import * as Data_Array from "../Data.Array/index.js";
import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
export function fire(state) {
  return () => {
    const current = state.value;
    if (current.tag === "Left") {
      const { _1: waiters } = current;
      const $value = {
        tag: "Right",
        _1: Data_Unit.unit
      };
      state.value = $value;
      return (/* @__PURE__ */ Data_Foldable.sequence_(Effect.applicativeEffect)(Data_Foldable.foldableArray)(waiters))();
    }
    if (current.tag === "Right") {
      return Data_Unit.unit;
    }
    throw new Error("Pattern match failure");
  };
}
export function wait(state) {
  return Iris_Effect_Async.async((callback) => {
    return () => {
      const current = state.value;
      $case: {
        if (current.tag === "Left") {
          const { _1: waiters } = current;
          const $value = {
            tag: "Left",
            _1: Data_Array.snoc(waiters)(callback({
              tag: "Right",
              _1: Data_Unit.unit
            }))
          };
          state.value = $value;
          break $case;
        }
        if (current.tag === "Right") {
          callback({
            tag: "Right",
            _1: Data_Unit.unit
          })();
          break $case;
        }
        throw new Error("Pattern match failure");
      }
      return (environment) => ({
        tag: "Pure",
        _1: Data_Unit.unit
      });
    };
  });
}
const $new = /* @__PURE__ */ (() => {
  const $function = (value) => value;
  const $value = {
    tag: "Left",
    _1: []
  };
  return () => {
    return $function({ value: $value });
  };
})();
export { $new as "new" };
