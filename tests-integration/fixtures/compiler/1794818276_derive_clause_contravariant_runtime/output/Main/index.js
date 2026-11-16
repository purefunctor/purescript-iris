import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
import * as Data_Profunctor from "../Data.Profunctor/index.js";
import * as Data_Show from "../Data.Show/index.js";
export const contravariantPredicate = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      const field0 = value;
      return (argument) => field0($function(argument));
    };
  } };
})();
export const profunctorTransform = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        const field0 = value;
        return (argument) => secondFunction(field0(firstFunction(argument)));
      };
    };
  } };
})();
export const predicate = /* @__PURE__ */ (() => {
  const $scrutinee = /* @__PURE__ */ Data_Functor_Contravariant.cmap(contravariantPredicate)((section49) => section49 + (4 | 0) | 0)((section60) => section60 > (7 | 0));
  const run = $scrutinee;
  return run(5 | 0) && !run(2 | 0);
})();
export const transformed = /* @__PURE__ */ (() => {
  const $scrutinee = /* @__PURE__ */ Data_Profunctor.dimap(profunctorTransform)((section96) => section96 + (3 | 0) | 0)((section103) => section103 + "!")(/* @__PURE__ */ Data_Show.show(Data_Show.showInt));
  const run = $scrutinee;
  return run(8 | 0);
})();
