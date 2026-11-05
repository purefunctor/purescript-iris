export function lookup(hasServiceServicesDict) {
  return hasServiceServicesDict.getService;
}
export function inferred(services) {
  return (environment) => {
    const $scrutinee = /* @__PURE__ */ lookup({ getService: services["Services.Console"] })(environment);
    const value = $scrutinee;
    return value;
  };
}
export function multiple(services) {
  return (environment) => ({
    console: /* @__PURE__ */ lookup({ getService: services["Services.Console"] })(environment),
    http: /* @__PURE__ */ lookup({ getService: services["Services.Http"] })(environment)
  });
}
export function delayed(services) {
  return /* @__PURE__ */ lookup({ getService: services["Services.Console"] });
}
export const inferredValue = /* @__PURE__ */ inferred({ "Services.Console": (environment) => environment.logger })({ logger: 37 | 0 });
export const multipleValue = /* @__PURE__ */ multiple({
  "Services.Http": (environment) => environment.network,
  "Services.Console": (environment$1) => environment$1.logger
})({
  logger: 19 | 0,
  network: 23 | 0
});
export const delayedValue = /* @__PURE__ */ delayed({ "Services.Console": (environment) => environment.logger })({
  network: 4 | 0,
  logger: 61 | 0
});
