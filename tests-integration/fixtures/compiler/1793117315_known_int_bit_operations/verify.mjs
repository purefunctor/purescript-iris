import { ok, strictEqual } from "node:assert/strict";
import * as Data_Int_Bits from "./output/Data.Int.Bits/index.js";
import * as Main from "./output/Main/index.js";

const integers = [-2147483648, -65536, -255, -2, -1, 0, 1, 2, 31, 32, 33, 255, 65535, 2147483647];

const sameValue = (actual, expected, message) => ok(Object.is(actual, expected), `${message}: ${actual} !== ${expected}`);

for (const [name, library] of [
  ["bitAnd", Data_Int_Bits.and],
  ["bitOr", Data_Int_Bits.or],
  ["bitXor", Data_Int_Bits.xor],
  ["shiftLeft", Data_Int_Bits.shl],
  ["shiftRight", Data_Int_Bits.shr],
  ["zeroFillShiftRight", Data_Int_Bits.zshr],
]) {
  for (const left of integers) {
    for (const right of integers) {
      sameValue(Main[name](left)(right), library(left)(right), `${name} ${left} ${right}`);
    }
  }
}

for (const value of integers) {
  sameValue(Main.bitComplement(value), Data_Int_Bits.complement(value), `bitComplement ${value}`);
}

strictEqual(Main.zeroFillShiftRight(-1)(0), 4294967295);
strictEqual(Main.lowByte(-1), 255);
strictEqual(Main.partiallyAppliedAnd(-1), 15);
