import * as Data_Semiring from "../Data.Semiring/index.js";
import * as Effect_Ref from "../Effect.Ref/index.js";
export function newCounter(value) {
  return Effect_Ref.new(value);
}
export function readCounter(counter) {
  return Effect_Ref.read(counter);
}
export function resetCounter(counter) {
  return Effect_Ref.write(0 | 0)(counter);
}
export function incrementCounter(counter) {
  return Effect_Ref.modify((section98) => section98 + (1 | 0) | 0)(counter);
}
export function addToField(step) {
  return (state) => {
    return Effect_Ref.modify(/* @__PURE__ */ Data_Semiring.add(Data_Semiring.semiringInt)(step))(state.counter);
  };
}
export function countTwice(value) {
  const $action = Effect_Ref.new(value);
  return () => {
    const counter = $action();
    const $action$1 = Effect_Ref.modify((section163) => section163 * (2 | 0) | 0)(counter);
    const $int = $action$1();
    const $action$2 = Effect_Ref.write(value + (1 | 0) | 0)(counter);
    const $unit = $action$2();
    const $action$3 = Effect_Ref.modify((section190) => section190 * (10 | 0) | 0)(counter);
    const next = $action$3();
    const $action$4 = Effect_Ref.read(counter);
    const current = $action$4();
    const $value = next + current | 0;
    return $value;
  };
}
