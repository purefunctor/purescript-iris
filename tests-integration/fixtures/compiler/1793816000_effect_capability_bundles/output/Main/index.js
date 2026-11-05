export function reading(hasFirstServicesDict) {
  return (hasSecondServicesDict) => {
    return (services) => {
      const $scrutinee = hasFirstServicesDict.getService(services);
      const $scrutinee$1 = hasSecondServicesDict.getService(services);
      const first = $scrutinee;
      const second = $scrutinee$1;
      return (first * (100 | 0) | 0) + second | 0;
    };
  };
}
export function inferred(hasFirstDict) {
  return (hasSecondDict) => {
    return (services) => /* @__PURE__ */ reading(hasFirstDict)(hasSecondDict)(services);
  };
}
export function ranked(hasFirstServicesDict) {
  return (hasSecondServicesDict) => {
    return (action) => /* @__PURE__ */ action(hasFirstServicesDict)(hasSecondServicesDict);
  };
}
export function tagged(hasFirstServicesDict) {
  return (hasTaggedValueServicesDict) => {
    return (hasSecondServicesDict) => {
      return (services) => {
        const $scrutinee = hasTaggedValueServicesDict.getService(services);
        const value = $scrutinee;
        return value;
      };
    };
  };
}
export const readingValue = /* @__PURE__ */ reading({ getService: ($services) => $services.first })({ getService: ($services) => $services.second })({
  first: 3 | 0,
  second: 7 | 0
});
export const inferredValue = /* @__PURE__ */ inferred({ getService: ($services) => $services.red })({ getService: ($services) => $services.blue })({
  red: 11 | 0,
  blue: 13 | 0
});
export const rankedValue = /* @__PURE__ */ ranked({ getService: ($services) => $services.first })({ getService: ($services) => $services.second })((hasFirstOtherDict) => (hasSecondOtherDict) => /* @__PURE__ */ reading(hasFirstOtherDict)(hasSecondOtherDict))({
  second: 2 | 0,
  first: 5 | 0
});
export const taggedValue = /* @__PURE__ */ tagged({ getService: ($services) => $services.first })({ getService: ($services) => $services.tagged })({ getService: ($services) => $services.second })({
  first: 0 | 0,
  second: 0 | 0,
  tagged: "tagged"
});
