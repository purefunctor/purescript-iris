import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Fiber from "../Iris.Effect.Fiber/index.js";
export function whenConfigured(configuration) {
  return (reference) => {
    const asyncArgument = configuration.enabled;
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const asyncArgument$1 = $effect;
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          let value;
          if (asyncArgument) {
            value = asyncArgument$1();
          } else {
            value = Data_Unit.unit;
          }
          return {
            tag: "Pure",
            _1: value
          };
        }
      };
    };
  };
}
export function unlessConfigured(configuration) {
  return (reference) => {
    const asyncArgument = !configuration.enabled;
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const asyncArgument$1 = $effect;
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          let value;
          if (asyncArgument) {
            value = asyncArgument$1();
          } else {
            value = Data_Unit.unit;
          }
          return {
            tag: "Pure",
            _1: value
          };
        }
      };
    };
  };
}
export function failWhen(condition) {
  return (environment) => {
    return {
      tag: "Run",
      _1: () => {
        if (condition) {
          return {
            tag: "Throw",
            _1: "failure"
          };
        } else {
          return {
            tag: "Pure",
            _1: Data_Unit.unit
          };
        }
      }
    };
  };
}
export function run(program) {
  return (callback) => {
    const $action = Iris_Effect_Fiber.newRuntime;
    return () => {
      const runtime = $action();
      const $action$1 = Iris_Effect_Fiber.makeFiber(runtime)(Iris_Effect_Async.toStep({})(program));
      const fiber = $action$1();
      return (/* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Fiber.joinFiber(fiber)(callback)))();
    };
  };
}
