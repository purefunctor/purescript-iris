import * as Data_Show from "../Data.Show/index.js";
export const results = /* @__PURE__ */ (() => {
  let ref = 1 | 0;
  const written = ref = 2 | 0;
  const modified = ref = ref + (10 | 0) | 0;
  const state = ref;
  const $modified = {
    state: state * (2 | 0) | 0,
    value: /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(state)
  };
  ref = $modified.state;
  const modifiedValue = $modified.value;
  const final = ref;
  const $value = {
    written,
    modified,
    modifiedValue,
    final
  };
  return $value;
})();
