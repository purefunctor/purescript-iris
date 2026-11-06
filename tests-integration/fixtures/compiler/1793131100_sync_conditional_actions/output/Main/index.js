import * as Iris_Effect_Sync from "../Iris.Effect.Sync/index.js";
export function whenConfigured(configuration) {
  return (reference) => {
    const syncArgument = configuration.enabled;
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const syncArgument$1 = $effect;
    return (environment) => {
      return () => {
        if (syncArgument) {
          syncArgument$1();
        }
        return;
      };
    };
  };
}
export function unlessConfigured(configuration) {
  return (reference) => {
    const syncArgument = !configuration.enabled;
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const syncArgument$1 = $effect;
    return (environment) => {
      return () => {
        if (syncArgument) {
          syncArgument$1();
        }
        return;
      };
    };
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
        const $value = current + (1 | 0) | 0;
        const $effect$1 = () => {
          reference.value = $value;
          return undefined;
        };
        const syncArgument$1 = $effect$1;
        const $condition = current < limit;
        if ($condition) {
          syncArgument$1();
        }
        const $effect$2 = () => {
          reference.value = 0 | 0;
          return undefined;
        };
        const syncArgument$2 = $effect$2;
        const $condition$1 = !(current < limit);
        if ($condition$1) {
          syncArgument$2();
        }
        const $effect$3 = () => {
          return reference.value;
        };
        const syncArgument$3 = $effect$3;
        return syncArgument$3();
      };
    };
  };
}
export const run = Iris_Effect_Sync.run({});
