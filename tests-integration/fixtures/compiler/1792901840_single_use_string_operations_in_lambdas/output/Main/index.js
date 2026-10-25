import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
export function sharedAppend(value) {
  const doubled = /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupString)(value)(value);
  return ($unit) => doubled;
}
export function sharedEqual(left) {
  return (right) => {
    const matches = /* @__PURE__ */ Data_Eq.eq(Data_Eq.eqString)(left)(right);
    return (value) => matches ? value : 0 | 0;
  };
}
