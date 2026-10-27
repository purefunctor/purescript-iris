import * as Data_Unit from "../Data.Unit/index.js";
export function namedContinuation($unit) {
  return () => {
    return 42 | 0;
  };
}
export function namedBind($unit) {
  const $value = Data_Unit.unit;
  return () => {
    return namedContinuation($value)();
  };
}
export function runNamedBind($unit) {
  return namedBind(Data_Unit.unit)();
}
