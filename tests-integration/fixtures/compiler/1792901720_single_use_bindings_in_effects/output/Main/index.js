import * as Data_Unit from "../Data.Unit/index.js";
export function sharedEffect(left) {
  return (right) => {
    const $value = Data_Unit.unit;
    return () => {
      const $value$1 = left + right | 0;
      return $value$1;
    };
  };
}
export function runningBind(left) {
  return (right) => {
    const $value = Data_Unit.unit;
    return () => {
      const $value$1 = left + right | 0;
      return $value$1;
    };
  };
}
