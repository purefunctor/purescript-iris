import * as Data_String_CodePoints from "../Data.String.CodePoints/index.js";
export function unknown(string) {
  return Data_String_CodePoints.toCodePointArray(string);
}
export const ascii = [
  97 | 0,
  98 | 0,
  99 | 0
];
export const astral = [
  97 | 0,
  119808 | 0,
  98 | 0
];
export const loneSurrogate = [55296 | 0, 120 | 0];
export const empty = [];
export const reexported = [97 | 0, 98 | 0];
