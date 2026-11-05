import * as Iris_Effect from "../Iris.Effect/index.js";
export const console = /* @__PURE__ */ Iris_Effect.getService({ getService: ($services) => $services.logger });
export const http = /* @__PURE__ */ Iris_Effect.getService({ getService: ($services) => $services.network });
export const misleading = /* @__PURE__ */ Iris_Effect.getService({ getService: ($services) => $services.http });
export const aliased = /* @__PURE__ */ Iris_Effect.getService({ getService: ($services) => $services.renamed });
export const consoleValue = console({
  logger: 19 | 0,
  network: 23 | 0
});
export const httpValue = http({
  logger: 19 | 0,
  network: 23 | 0
});
export const misleadingValue = misleading({
  console: 9 | 0,
  http: 42 | 0
});
export const aliasedValue = aliased({ renamed: 51 | 0 });
