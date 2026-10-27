import * as Data_Show from "../Data.Show/index.js";
export function increment(ref) {
  return () => {
    ref.value = ref.value + (1 | 0) | 0;
  };
}
export const results = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 1 | 0 };
    ref.value = 2 | 0;
    const written = void 0;
    const modified = ref.value = ref.value + (10 | 0) | 0;
    ref.value = ref.value * (2 | 0) | 0;
    const modifiedUnit = void 0;
    const state = ref.value;
    const $modified = {
      state: state + (1 | 0) | 0,
      value: /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(state)
    };
    ref.value = $modified.state;
    const modifiedValue = $modified.value;
    const final = ref.value;
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
