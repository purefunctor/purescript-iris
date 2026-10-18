export const Wrapper = ($value0) => ({
  tag: "Wrapper",
  _1: $value0
});
export function equal(dictionary) {
  return dictionary.equal;
}
export function convert(dictionary) {
  return dictionary.convert;
}
export function available(dictionary) {
  return dictionary.available;
}
export function genericEqual(equalValueDict) {
  return (left) => (right) => /* @__PURE__ */ equal(equalValueDict)(left)(right);
}
export function arrayEqual(equalArrayValueDict) {
  return (left) => (right) => /* @__PURE__ */ equal(equalArrayValueDict)(left)(right);
}
export function wrapperEqual(equalWrapperValueDict) {
  return (left) => (right) => /* @__PURE__ */ equal(equalWrapperValueDict)(left)(right);
}
export function concreteEqual(equalIntDict) {
  return (left) => (right) => /* @__PURE__ */ equal(equalIntDict)(left)(right);
}
export function convertToInt(convertValueIntDict) {
  return (value) => /* @__PURE__ */ convert(convertValueIntDict)(value);
}
export function distinctEqual(equalLeftDict) {
  return (equalRightDict) => {
    return (left1) => (left2) => (right1) => (right2) => ({
      left: /* @__PURE__ */ equal(equalLeftDict)(left1)(left2),
      right: /* @__PURE__ */ equal(equalRightDict)(right1)(right2)
    });
  };
}
export function duplicateEqual(equalValueDict) {
  return (equalValueDict$1) => {
    return (left) => (right) => /* @__PURE__ */ equal(equalValueDict$1)(left)(right);
  };
}
export function parameterCollision(equalValueDict) {
  return (equalValueDict$1) => (left) => (right) => equalValueDict$1 ? /* @__PURE__ */ equal(equalValueDict)(left)(right) : false;
}
export function isAvailable(availableDict) {
  return /* @__PURE__ */ available(availableDict);
}
