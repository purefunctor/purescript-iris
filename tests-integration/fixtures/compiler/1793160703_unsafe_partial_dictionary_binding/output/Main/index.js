import * as Data_Show from "../Data.Show/index.js";
import * as Partial from "../Partial/index.js";
export function positive(partialDict) {
  const $closure = (value) => {
    if (value > (0 | 0)) {
      return value;
    }
    throw new Error("Pattern match failure");
  };
  return $closure;
}
export function unusedDictionary(value) {
  return value + (1 | 0) | 0;
}
export function forwardedDictionary(value) {
  return /* @__PURE__ */ positive({})(value);
}
export function forwardedCrash(value) {
  return /* @__PURE__ */ Partial.crashWith({})("crashed with " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(value));
}
export const extraArguments = ((left) => (right) => /* @__PURE__ */ positive({})(left) + right | 0)(10 | 0);
