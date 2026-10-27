import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Show from "../Data.Show/index.js";
export const results = /* @__PURE__ */ (() => {
  const $function = Control_Monad_ST_Internal.run;
  const $action = Control_Monad_ST_Internal.new(1 | 0);
  const $effect = () => {
    const ref = $action();
    const $action$1 = Control_Monad_ST_Internal.write(2 | 0)(ref);
    const written = $action$1();
    const $action$2 = Control_Monad_ST_Internal.modify((section63) => section63 + (10 | 0) | 0)(ref);
    const modified = $action$2();
    const $action$3 = Control_Monad_ST_Internal["modify'"]((state) => ({
      state: state * (2 | 0) | 0,
      value: /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(state)
    }))(ref);
    const modifiedValue = $action$3();
    const $action$4 = Control_Monad_ST_Internal.read(ref);
    const final = $action$4();
    const $value = {
      written,
      modified,
      modifiedValue,
      final
    };
    return $value;
  };
  return $function($effect);
})();
