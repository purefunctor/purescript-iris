import * as Data_Either from "../Data.Either/index.js";
import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Function from "../Data.Function/index.js";
import * as Data_Lens_Internal_Market from "../Data.Lens.Internal.Market/index.js";
import * as Data_Lens_Prism from "../Data.Lens.Prism/index.js";
import * as Data_Lens_Prism_Maybe from "../Data.Lens.Prism.Maybe/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
export function test(choicePDict) {
  return /* @__PURE__ */ Data_Lens_Prism.prism(Data_Maybe.Just)(Data_Maybe.maybe({
    tag: "Left",
    _1: "Nothing"
  })(Data_Either.Right))(choicePDict);
}
function test_(choicePDict) {
  return /* @__PURE__ */ Data_Lens_Prism.prism(Data_Maybe.Just)(Data_Maybe.maybe({
    tag: "Left",
    _1: "Nothing"
  })(Data_Either.Right))(choicePDict);
}
export function test2(choicePDict) {
  return /* @__PURE__ */ Data_Lens_Prism.prism(Data_Function.const("Nothing"))(Data_Maybe.maybe({
    tag: "Right",
    _1: Data_Unit.unit
  })(Data_Function.const({
    tag: "Left",
    _1: "Nothing"
  })))(choicePDict);
}
const eqMaybeEqIntDict = /* @__PURE__ */ Data_Maybe.eqMaybe(Data_Eq.eqInt);
const eqArrayEqIntDict = /* @__PURE__ */ Data_Eq.eqArray(Data_Eq.eqInt);
const eqArrayEqMaybeEqIntDict = /* @__PURE__ */ Data_Eq.eqArray(eqMaybeEqIntDict);
const eqEitherMaybeIntUnitDict = /* @__PURE__ */ Data_Either.eqEither(eqMaybeEqIntDict)(Data_Eq.eqUnit);
const eqEitherArrayMaybeIntDict = /* @__PURE__ */ Data_Either.eqEither(eqArrayEqMaybeEqIntDict)(eqArrayEqIntDict);
const eqEitherMaybeIntUnitDictEq = /* @__PURE__ */ Data_Eq.eq(eqEitherMaybeIntUnitDict);
const eqEitherArrayMaybeIntDictEq = /* @__PURE__ */ Data_Eq.eq(eqEitherArrayMaybeIntDict);
export const results = [
  /* @__PURE__ */ eqEitherArrayMaybeIntDictEq(Data_Lens_Prism.matching(/* @__PURE__ */ Data_Lens_Prism.below(Data_Traversable.traversableArray)(/* @__PURE__ */ test(Data_Lens_Internal_Market.choiceMarket))(Data_Lens_Internal_Market.choiceMarket))([{
    tag: "Just",
    _1: 3 | 0
  }, {
    tag: "Just",
    _1: 8 | 0
  }]))({
    tag: "Right",
    _1: [3 | 0, 8 | 0]
  }),
  /* @__PURE__ */ eqEitherArrayMaybeIntDictEq(Data_Lens_Prism.matching(/* @__PURE__ */ Data_Lens_Prism.below(Data_Traversable.traversableArray)(/* @__PURE__ */ test_(Data_Lens_Internal_Market.choiceMarket))(Data_Lens_Internal_Market.choiceMarket))([{
    tag: "Just",
    _1: 3 | 0
  }, {
    tag: "Just",
    _1: 8 | 0
  }]))({
    tag: "Right",
    _1: [3 | 0, 8 | 0]
  }),
  /* @__PURE__ */ eqEitherArrayMaybeIntDictEq(Data_Lens_Prism.matching(/* @__PURE__ */ Data_Lens_Prism.below(Data_Traversable.traversableArray)(/* @__PURE__ */ Data_Lens_Prism_Maybe._Just(Data_Lens_Internal_Market.choiceMarket))(Data_Lens_Internal_Market.choiceMarket))([{
    tag: "Just",
    _1: 3 | 0
  }, {
    tag: "Just",
    _1: 8 | 0
  }]))({
    tag: "Right",
    _1: [3 | 0, 8 | 0]
  }),
  /* @__PURE__ */ eqEitherArrayMaybeIntDictEq(Data_Lens_Prism.matching(/* @__PURE__ */ Data_Lens_Prism.below(Data_Traversable.traversableArray)(/* @__PURE__ */ test(Data_Lens_Internal_Market.choiceMarket))(Data_Lens_Internal_Market.choiceMarket))([{
    tag: "Just",
    _1: 3 | 0
  }, "Nothing"]))({
    tag: "Left",
    _1: [{
      tag: "Just",
      _1: 3 | 0
    }, "Nothing"]
  }),
  /* @__PURE__ */ eqEitherMaybeIntUnitDictEq(Data_Lens_Prism.matching(/* @__PURE__ */ test2(Data_Lens_Internal_Market.choiceMarket))("Nothing"))({
    tag: "Right",
    _1: Data_Unit.unit
  }),
  /* @__PURE__ */ eqEitherMaybeIntUnitDictEq(Data_Lens_Prism.matching(/* @__PURE__ */ test2(Data_Lens_Internal_Market.choiceMarket))({
    tag: "Just",
    _1: 8 | 0
  }))({
    tag: "Left",
    _1: "Nothing"
  }),
  /* @__PURE__ */ eqEitherMaybeIntUnitDictEq(Data_Lens_Prism.matching(/* @__PURE__ */ Data_Lens_Prism_Maybe._Nothing(Data_Lens_Internal_Market.choiceMarket))("Nothing"))({
    tag: "Right",
    _1: Data_Unit.unit
  })
];
export { test_ as "test'" };
