import * as Control_Extend from "../Control.Extend/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
const extendFunctionStringDict = /* @__PURE__ */ Control_Extend.extendFn(Data_Semigroup.semigroupString);
const semigroupStringDictAppend = /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupString);
const extendFunctionStringDictExtend = /* @__PURE__ */ Control_Extend.extend(extendFunctionStringDict);
export const test = /* @__PURE__ */ extendFunctionStringDictExtend((f) => f("B"))((s) => /* @__PURE__ */ semigroupStringDictAppend(s)("!"))("A");
const test_ = /* @__PURE__ */ extendFunctionStringDictExtend((f) => f("B"))((s) => /* @__PURE__ */ semigroupStringDictAppend(s)("!"))("A");
export { test_ as "test'" };
