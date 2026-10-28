import * as Data_Show from "../Data.Show/index.js";
export function increment(ref) {
  return () => {
    ref.value = ref.value + (1 | 0) | 0;
  };
}
export const results = /* @__PURE__ */ (() => {
  return () => {
    let ref = 1 | 0;
    ref = 2 | 0;
    const written = void 0;
    const modified = ref = ref + (10 | 0) | 0;
    ref = ref * (2 | 0) | 0;
    const modifiedUnit = void 0;
    const state = ref;
    const $modified = {
      state: state + (1 | 0) | 0,
      value: /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(state)
    };
    ref = $modified.state;
    const modifiedValue = $modified.value;
    const final = ref;
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
