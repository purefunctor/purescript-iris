import * as Data_Generic_Rep from "../Data.Generic.Rep/index.js";
import * as Data_Newtype from "../Data.Newtype/index.js";
import * as $runtime from "../runtime.js";
export const Empty = "Empty";
export const Single = ($value0) => ({
  tag: "Single",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function roundTrip(value) {
  return /* @__PURE__ */ Data_Generic_Rep.to(genericChoiceSumConstructorNoArgumentsSumConstructorArgumentIntConstructorProductArgumentIntArgumentInt)(/* @__PURE__ */ Data_Generic_Rep.from(genericChoiceSumConstructorNoArgumentsSumConstructorArgumentIntConstructorProductArgumentIntArgumentInt)(value));
}
const $lazy_genericVoidNoConstructors = $runtime.binding("genericVoidNoConstructors", () => {
  return {
    to: (value) => {
      return /* @__PURE__ */ Data_Generic_Rep.to($lazy_genericVoidNoConstructors())(value);
    },
    from: (value$1) => {
      return /* @__PURE__ */ Data_Generic_Rep.from($lazy_genericVoidNoConstructors())(value$1);
    }
  };
});
export const newtypeIdentifierInt = { Coercible0: () => ({}) };
export const wrapped = 42 | 0;
export const unwrapped = /* @__PURE__ */ Data_Newtype.unwrap(newtypeIdentifierInt)(wrapped);
export const genericChoiceSumConstructorNoArgumentsSumConstructorArgumentIntConstructorProductArgumentIntArgumentInt = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      if (representation.tag === "Inl" && representation._1 === "NoArguments") {
        return "Empty";
      }
      if (representation.tag === "Inr" && representation._1.tag === "Inl") {
        const { _1: field0 } = representation._1;
        return {
          tag: "Single",
          _1: field0
        };
      }
      if (representation.tag === "Inr" && representation._1.tag === "Inr" && representation._1._1.tag === "Product") {
        const { _1: field0$1, _2: field1 } = representation._1._1;
        return {
          tag: "Pair",
          _1: field0$1,
          _2: field1
        };
      }
      throw new Error("Pattern match failure");
    },
    from: (value) => {
      if (value === "Empty") {
        return {
          tag: "Inl",
          _1: "NoArguments"
        };
      }
      if (value.tag === "Single") {
        const { _1: field0$2 } = value;
        return {
          tag: "Inr",
          _1: {
            tag: "Inl",
            _1: field0$2
          }
        };
      }
      if (value.tag === "Pair") {
        const { _1: field0$3, _2: field1$1 } = value;
        return {
          tag: "Inr",
          _1: {
            tag: "Inr",
            _1: {
              tag: "Product",
              _1: field0$3,
              _2: field1$1
            }
          }
        };
      }
      throw new Error("Pattern match failure");
    }
  };
})();
export const emptyRoundTrip = roundTrip("Empty");
export const singleRoundTrip = roundTrip({
  tag: "Single",
  _1: 6 | 0
});
export const pairRoundTrip = roundTrip({
  tag: "Pair",
  _1: 7 | 0,
  _2: 8 | 0
});
export const genericVoidNoConstructors = $lazy_genericVoidNoConstructors();
