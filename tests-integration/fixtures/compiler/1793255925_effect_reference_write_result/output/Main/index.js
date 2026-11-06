import * as Data_Unit from "../Data.Unit/index.js";
export function writeResult(reference) {
  return () => {
    reference.value = 7 | 0;
    return Data_Unit.unit;
  };
}
export function boundWriteResult(reference) {
  return () => {
    reference.value = 8 | 0;
    return Data_Unit.unit;
  };
}
export function localWriteResult(value) {
  return () => {
    let reference = value;
    const $value = value + (1 | 0) | 0;
    reference = $value;
    return Data_Unit.unit;
  };
}
