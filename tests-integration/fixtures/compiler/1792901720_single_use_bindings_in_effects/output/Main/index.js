import * as Data_Unit from "../Data.Unit/index.js";
export function sharedEffect(left) {
  return (right) => {
    const total = left + right | 0;
    const $value = Data_Unit.unit;
    return () => {
      return total;
    };
  };
}
export function runningBind(left) {
  return (right) => {
    const $value = Data_Unit.unit;
    return () => {
      return left + right | 0;
    };
  };
}
