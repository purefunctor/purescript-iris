import * as Data_Unit from "../Data.Unit/index.js";
import * as Iris_Effect_Sync from "../Iris.Effect.Sync/index.js";
export function whenConfigured(configuration) {
  return (reference) => {
    const syncArgument = configuration.enabled;
    const $value = configuration.value;
    const syncArgument$1 = () => {
      reference.value = $value;
      return Data_Unit.unit;
    };
    return (environment) => {
      return () => {
        if (syncArgument) {
          syncArgument$1();
        }
        return Data_Unit.unit;
      };
    };
  };
}
export function unlessConfigured(configuration) {
  return (reference) => {
    const syncArgument = !configuration.enabled;
    const $value = configuration.value;
    const syncArgument$1 = () => {
      reference.value = $value;
      return Data_Unit.unit;
    };
    return (environment) => {
      return () => {
        if (syncArgument) {
          syncArgument$1();
        }
        return Data_Unit.unit;
      };
    };
  };
}
export function step(reference) {
  return (limit) => {
    const syncArgument = () => {
      return reference.value;
    };
    return (environment) => {
      return () => {
        const current = syncArgument();
        const $value = current + (1 | 0) | 0;
        const syncArgument$1 = () => {
          reference.value = $value;
          return Data_Unit.unit;
        };
        const $condition = current < limit;
        if ($condition) {
          syncArgument$1();
        }
        const syncArgument$2 = () => {
          reference.value = 0 | 0;
          return Data_Unit.unit;
        };
        const $condition$1 = !(current < limit);
        if ($condition$1) {
          syncArgument$2();
        }
        const syncArgument$3 = () => {
          return reference.value;
        };
        return syncArgument$3();
      };
    };
  };
}
export const run = Iris_Effect_Sync.run({});
