export function warningWithoutError($boolean) {
  if ($boolean === true) {
    return 1 | 0;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function warningAfterError($boolean) {
  if ($boolean === true) {
    return "one";
  } else {
    throw new Error("Pattern match failure");
  }
}
export const independentMismatches = {
  first: "first",
  second: true
};
export const signatureAndEquation = (() => {
  throw new Error("Generated code reached a source error");
})();
