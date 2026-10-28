import * as Effect_Ref from "../Effect.Ref/index.js";
export function readBeforeWrite(value) {
  return () => {
    const reference = { value };
    const before = reference.value;
    const $value = value + (4 | 0) | 0;
    const $unit = reference.value = $value;
    const after = reference.value;
    const $value$1 = (before * (10 | 0) | 0) + after | 0;
    return $value$1;
  };
}
export function sumTwo(left) {
  return (right) => {
    return () => {
      const first = { value: left };
      const second = { value: right };
      let $int;
      const section120 = first.value;
      const $value = section120 + (1 | 0) | 0;
      $int = first.value = $value;
      const a = first.value;
      const b = second.value;
      const $value$1 = a + b | 0;
      return $value$1;
    };
  };
}
export function escapes(value) {
  return () => {
    const reference = { value };
    const $value = value * (2 | 0) | 0;
    const $unit = reference.value = $value;
    return reference;
  };
}
export function passedToFunction(value) {
  return () => {
    const reference = { value };
    const $action = Effect_Ref.modify_((section218) => section218 + (1 | 0) | 0)(reference);
    const $unit = $action();
    return reference.value;
  };
}
