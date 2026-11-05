import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Iris_Effect_Sync from "../Iris.Effect.Sync/index.js";
export function whenConfigured(configuration) {
  return (reference) => {
    const $function = Control_Applicative.when(Iris_Effect_Sync.applicativeSync)(configuration.enabled);
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const syncArgument = $effect;
    const $result = (environment) => syncArgument;
    return /* @__PURE__ */ $function($result);
  };
}
export function unlessConfigured(configuration) {
  return (reference) => {
    const $function = Control_Applicative.unless(Iris_Effect_Sync.applicativeSync)(configuration.enabled);
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const syncArgument = $effect;
    const $result = (environment) => syncArgument;
    return /* @__PURE__ */ $function($result);
  };
}
export function step(reference) {
  return (limit) => {
    const $effect = () => {
      return reference.value;
    };
    const syncArgument = $effect;
    return (environment) => {
      return () => {
        const current = syncArgument();
        const $function = Control_Applicative.when(Iris_Effect_Sync.applicativeSync)(current < limit);
        const $value = current + (1 | 0) | 0;
        const $effect$1 = () => {
          reference.value = $value;
          return undefined;
        };
        const syncArgument$2 = $effect$1;
        const $result = (environment$1) => syncArgument$2;
        const syncArgument$1 = /* @__PURE__ */ $function($result);
        const $action = syncArgument$1(environment);
        const $unit = $action();
        const $function$1 = Control_Applicative.unless(Iris_Effect_Sync.applicativeSync)(current < limit);
        const $effect$2 = () => {
          reference.value = 0 | 0;
          return undefined;
        };
        const syncArgument$4 = $effect$2;
        const $result$1 = (environment$2) => syncArgument$4;
        const syncArgument$3 = /* @__PURE__ */ $function$1($result$1);
        const $action$1 = syncArgument$3(environment);
        const $unit$1 = $action$1();
        const $effect$3 = () => {
          return reference.value;
        };
        const syncArgument$5 = $effect$3;
        return syncArgument$5();
      };
    };
  };
}
export const run = Iris_Effect_Sync.run({});
