export function reading(services) {
  return (services$1) => {
    const $scrutinee = services["Capabilities.First"](services$1);
    const $scrutinee$1 = services["Capabilities.Second"](services$1);
    const first = $scrutinee;
    const second = $scrutinee$1;
    return (first * (100 | 0) | 0) + second | 0;
  };
}
export function inferred(services) {
  return (services$1) => /* @__PURE__ */ reading(services)(services$1);
}
export function ranked(services) {
  return (action) => /* @__PURE__ */ action({ getService: services["Capabilities.First"] })({ getService: services["Capabilities.Second"] });
}
export function tagged(services) {
  return (hasTaggedValueServicesDict) => {
    return (services$1) => {
      const $scrutinee = hasTaggedValueServicesDict.getService(services$1);
      const value = $scrutinee;
      return value;
    };
  };
}
export const readingValue = /* @__PURE__ */ reading({
  "Capabilities.First": (environment) => environment.first,
  "Capabilities.Second": (environment$1) => environment$1.second
})({
  first: 3 | 0,
  second: 7 | 0
});
export const inferredValue = /* @__PURE__ */ inferred({
  "Capabilities.First": (environment) => environment.red,
  "Capabilities.Second": (environment$1) => environment$1.blue
})({
  red: 11 | 0,
  blue: 13 | 0
});
export const rankedValue = /* @__PURE__ */ ranked({
  "Capabilities.First": (environment) => environment.first,
  "Capabilities.Second": (environment$1) => environment$1.second
})((hasFirstOtherDict) => (hasSecondOtherDict) => /* @__PURE__ */ reading({
  "Capabilities.First": hasFirstOtherDict.getService,
  "Capabilities.Second": hasSecondOtherDict.getService
}))({
  second: 2 | 0,
  first: 5 | 0
});
export const taggedValue = /* @__PURE__ */ tagged({
  "Capabilities.First": (environment) => environment.first,
  "Capabilities.Second": (environment$1) => environment$1.second
})({ getService: (environment$2) => environment$2.tagged })({
  first: 0 | 0,
  second: 0 | 0,
  tagged: "tagged"
});
