export const Left = "Left";
export const Right = "Right";
export function before(dictionary) {
  return dictionary.before;
}
export function runComparison(comparison) {
  return /* @__PURE__ */ comparison(beforeMarker)("Left")("Right");
}
export const beforeMarker = /* @__PURE__ */ (() => {
  return { before: ($marker) => {
    return ($marker$1) => {
      if ($marker === "Left" && $marker$1 === "Right") {
        return true;
      }
      return false;
    };
  } };
})();
export const result = runComparison((beforeValueDict) => (left) => (right) => /* @__PURE__ */ before(beforeValueDict)(left)(right));
