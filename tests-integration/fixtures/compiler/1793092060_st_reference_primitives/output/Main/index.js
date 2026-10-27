import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Show from "../Data.Show/index.js";
export const results = /* @__PURE__ */ (() => {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    const ref = { value: 1 | 0 };
    const written = ref.value = 2 | 0;
    const modified = ref.value = ref.value + (10 | 0) | 0;
    const state = ref.value;
    const $modified = {
      state: state * (2 | 0) | 0,
      value: /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(state)
    };
    ref.value = $modified.state;
    const modifiedValue = $modified.value;
    const final = ref.value;
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
