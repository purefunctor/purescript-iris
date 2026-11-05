import * as Iris_Effect from "../Iris.Effect/index.js";
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
    const $scrutinee = /* @__PURE__ */ Iris_Effect.getService(/* @__PURE__ */ bothServicesDict.Has0())(services);
    const $scrutinee$1 = /* @__PURE__ */ Iris_Effect.getService(/* @__PURE__ */ bothServicesDict.Has1())(services);
    const first = $scrutinee;
    const second = $scrutinee$1;
    return (first * (100 | 0) | 0) + second | 0;
  };
}
export const readingValue = /* @__PURE__ */ reading(/* @__PURE__ */ both({ getService: ($services) => $services.first })({ getService: ($services) => $services.second }))({
  first: 3 | 0,
  second: 7 | 0
});
