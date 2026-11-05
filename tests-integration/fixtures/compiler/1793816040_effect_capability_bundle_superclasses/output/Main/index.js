export function both(hasFirstServicesDict) {
  return (hasSecondServicesDict) => {
    return {
      Has0: () => hasFirstServicesDict,
      Has1: () => hasSecondServicesDict
    };
  };
}
export function reading(bothServicesDict) {
  return (services) => {
    const $scrutinee = (/* @__PURE__ */ bothServicesDict.Has0()).getService(services);
    const $scrutinee$1 = (/* @__PURE__ */ bothServicesDict.Has1()).getService(services);
    const first = $scrutinee;
    const second = $scrutinee$1;
    return (first * (100 | 0) | 0) + second | 0;
  };
}
export const readingValue = /* @__PURE__ */ reading(/* @__PURE__ */ both({ getService: ($services) => $services.first })({ getService: ($services) => $services.second }))({
  first: 3 | 0,
  second: 7 | 0
});
