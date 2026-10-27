import * as Data_Show from "../Data.Show/index.js";
import * as Effect_Ref from "../Effect.Ref/index.js";
export function increment(ref) {
  return Effect_Ref.modify_((count) => count + (1 | 0) | 0)(ref);
}
export const results = /* @__PURE__ */ (() => {
  const $action = Effect_Ref.new(1 | 0);
  return () => {
    const ref = $action();
    const $action$1 = Effect_Ref.write(2 | 0)(ref);
    const written = $action$1();
    const $action$2 = Effect_Ref.modify((section67) => section67 + (10 | 0) | 0)(ref);
    const modified = $action$2();
    const $action$3 = Effect_Ref.modify_((section81) => section81 * (2 | 0) | 0)(ref);
    const modifiedUnit = $action$3();
    const $action$4 = Effect_Ref["modify'"]((state) => ({
      state: state + (1 | 0) | 0,
      value: /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(state)
    }))(ref);
    const modifiedValue = $action$4();
    const $action$5 = Effect_Ref.read(ref);
    const final = $action$5();
    const $value = {
      written,
      modified,
      modifiedUnit,
      modifiedValue,
      final
    };
    return $value;
  };
})();
