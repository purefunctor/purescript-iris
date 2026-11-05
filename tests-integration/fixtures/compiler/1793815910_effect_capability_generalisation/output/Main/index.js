import * as Iris_Effect from "../Iris.Effect/index.js";
export function lookup(hasServiceServicesDict) {
  return /* @__PURE__ */ Iris_Effect.getService(hasServiceServicesDict);
}
export function inferred(hasConsoleDict) {
  return (environment) => {
    const $scrutinee = /* @__PURE__ */ lookup(hasConsoleDict)(environment);
    const value = $scrutinee;
    return value;
  };
}
export function multiple(hasHttpDict) {
  return (hasConsoleDict) => {
    return (environment) => ({
      console: /* @__PURE__ */ lookup(hasConsoleDict)(environment),
      http: /* @__PURE__ */ lookup(hasHttpDict)(environment)
    });
  };
}
export function delayed(hasConsoleRowDict) {
  return /* @__PURE__ */ lookup(hasConsoleRowDict);
}
export const inferredValue = /* @__PURE__ */ inferred({ getService: ($services) => $services.logger })({ logger: 37 | 0 });
export const multipleValue = /* @__PURE__ */ multiple({ getService: ($services) => $services.network })({ getService: ($services) => $services.logger })({
  logger: 19 | 0,
  network: 23 | 0
});
export const delayedValue = /* @__PURE__ */ delayed({ getService: ($services) => $services.logger })({
  network: 4 | 0,
  logger: 61 | 0
});
