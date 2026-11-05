import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Fiber from "../Iris.Effect.Fiber/index.js";
export function whenConfigured(configuration) {
  return (reference) => {
    const $function = Control_Applicative.when(Iris_Effect_Async.applicativeAsync)(configuration.enabled);
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const asyncArgument = $effect;
    const $result = (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const value = asyncArgument();
          return {
            tag: "Pure",
            _1: value
          };
        }
      };
    };
    return /* @__PURE__ */ $function($result);
  };
}
export function unlessConfigured(configuration) {
  return (reference) => {
    const $function = Control_Applicative.unless(Iris_Effect_Async.applicativeAsync)(configuration.enabled);
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    const asyncArgument = $effect;
    const $result = (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const value = asyncArgument();
          return {
            tag: "Pure",
            _1: value
          };
        }
      };
    };
    return /* @__PURE__ */ $function($result);
  };
}
export function failWhen(condition) {
  return /* @__PURE__ */ Control_Applicative.when(Iris_Effect_Async.applicativeAsync)(condition)((environment) => ({
    tag: "Throw",
    _1: "failure"
  }));
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
