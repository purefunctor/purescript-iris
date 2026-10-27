import * as Data_Int_Bits from "../Data.Int.Bits/index.js";
export function bitAnd(left) {
  return (right) => {
    return left & right;
  };
}
export function bitOr(left) {
  return (right) => {
    return left | right;
  };
}
export function bitXor(left) {
  return (right) => {
    return left ^ right;
  };
}
export function shiftLeft(value) {
  return (count) => {
    return value << count;
  };
}
export function shiftRight(value) {
  return (count) => {
    return value >> count;
  };
}
export function zeroFillShiftRight(value) {
  return (count) => {
    return value >>> count;
  };
}
export function bitComplement(value) {
  return ~value;
}
export function lowByte(value) {
  return value & (255 | 0) | ~(0 | 0) & (0 | 0);
}
export const partiallyAppliedAnd = Data_Int_Bits.and(15 | 0);
