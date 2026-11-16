import * as Data_Newtype from "../Data.Newtype/index.js";
export const Plain = ($value0) => ({
  tag: "Plain",
  _1: $value0
});
export const newtypeAttached = { Coercible0: () => ({}) };
export const newtypeStandalone = { Coercible0: () => ({}) };
export const result = /* @__PURE__ */ Data_Newtype.unwrap(newtypeAttached)(41 | 0) + /* @__PURE__ */ Data_Newtype.unwrap(newtypeStandalone)(1 | 0) | 0;
