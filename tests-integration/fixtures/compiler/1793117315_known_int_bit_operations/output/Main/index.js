import * as Data_Int_Bits from "../Data.Int.Bits/index.js";
export function bitAnd(left) {
  return (right) => {
    return Data_Int_Bits.and(left)(right);
  };
}
export function bitOr(left) {
  return (right) => {
    return Data_Int_Bits.or(left)(right);
  };
}
export function bitXor(left) {
  return (right) => {
    return Data_Int_Bits.xor(left)(right);
  };
}
export function shiftLeft(value) {
  return (count) => {
    return Data_Int_Bits.shl(value)(count);
  };
}
export function shiftRight(value) {
  return (count) => {
    return Data_Int_Bits.shr(value)(count);
  };
}
export function zeroFillShiftRight(value) {
  return (count) => {
    return Data_Int_Bits.zshr(value)(count);
  };
}
export function bitComplement(value) {
  return Data_Int_Bits.complement(value);
}
export function lowByte(value) {
  return Data_Int_Bits.or(Data_Int_Bits.and(value)(255 | 0))(Data_Int_Bits.and(Data_Int_Bits.complement(0 | 0))(0 | 0));
}
export const partiallyAppliedAnd = Data_Int_Bits.and(15 | 0);
