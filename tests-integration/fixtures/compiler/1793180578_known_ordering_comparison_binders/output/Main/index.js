import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
import * as Data_Show from "../Data.Show/index.js";
export function isLess(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ ordIntDictCompare(left)(right);
    if ($scrutinee === "LT") {
      return "less";
    }
    const other = $scrutinee;
    return "not less";
  };
}
export function describeOrdering(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ ordIntDictCompare(left)(right);
    if ($scrutinee === "LT") {
      return "less";
    }
    const other = $scrutinee;
    return /* @__PURE__ */ Data_Show.show(Data_Ordering.showOrdering)(other);
  };
}
const ordIntDictCompare = /* @__PURE__ */ Data_Ord.compare(Data_Ord.ordInt);
