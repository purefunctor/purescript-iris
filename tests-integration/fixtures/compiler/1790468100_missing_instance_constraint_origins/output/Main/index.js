import * as Data_Semiring from "../Data.Semiring/index.js";
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export function test2(value) {
  const $function = Data_Semiring.add;
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function($result)(value)(value);
}
function test2_(semiringDict) {
  return (value) => /* @__PURE__ */ Data_Semiring.add(semiringDict)(value)(value);
}
export function test3(semiringADict) {
  return (value) => /* @__PURE__ */ Data_Semiring.add(semiringADict)(value)(value);
}
export function combine(dictionary) {
  return dictionary.combine;
}
export function combineBox(semiringADict) {
  const $closure = ($box) => {
    if ($box.tag === "Box") {
      const { _1: left } = $box;
      return ($box$1) => {
        if ($box$1.tag === "Box") {
          const { _1: right } = $box$1;
          return {
            tag: "Box",
            _1: /* @__PURE__ */ Data_Semiring.add(semiringADict)(left)(right)
          };
        } else {
          throw new Error("Pattern match failure");
        }
      };
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return { combine: $closure };
}
export const test = (() => {
  const $function = Data_Semiring.add;
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function($result)("left")("right");
})();
const test_ = (() => {
  const $function = Data_Semiring.add;
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function($result)("left")("right");
})();
export const test4 = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ combine(/* @__PURE__ */ combineBox($result))({
    tag: "Box",
    _1: "left"
  })({
    tag: "Box",
    _1: "right"
  });
})();
export { test_ as "test'" };
export { test2_ as "test2'" };
