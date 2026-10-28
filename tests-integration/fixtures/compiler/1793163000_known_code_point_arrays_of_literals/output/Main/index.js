import * as Data_String_CodePoints from "../Data.String.CodePoints/index.js";
export function unknown(string) {
  return Data_String_CodePoints.toCodePointArray(string);
}
export const ascii = Data_String_CodePoints.toCodePointArray("abc");
export const astral = Data_String_CodePoints.toCodePointArray("a𝐀b");
export const loneSurrogate = Data_String_CodePoints.toCodePointArray("\ud800x");
export const empty = Data_String_CodePoints.toCodePointArray("");
export const reexported = Data_String_CodePoints.toCodePointArray("ab");
