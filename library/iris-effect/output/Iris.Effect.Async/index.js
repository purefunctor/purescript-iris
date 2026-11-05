import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Category from "../Control.Category/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect_Exception from "../Effect.Exception/index.js";
import * as Iris_Effect_Fiber from "../Iris.Effect.Fiber/index.js";
export function make(value) {
  return value;
}
export function toStep(environment) {
  return (program) => {
    return program(environment);
  };
}
export function sync(program) {
  return (environment) => ({
    tag: "Sync",
    _1: program(environment)
  });
}
export function liftEffect(effect) {
  return ($record) => ({
    tag: "Sync",
    _1: effect
  });
}
export function service(hasServiceServicesDict) {
  return (environment) => ({
    tag: "Pure",
    _1: hasServiceServicesDict.getService(environment)
  });
}
export function provide(environment) {
  return (program) => {
    return ($record) => program(environment);
  };
}
export function fail(error) {
  return ($record) => ({
    tag: "Throw",
    _1: error
  });
}
function $catch(program) {
  return (handler) => {
    return (environment) => Iris_Effect_Fiber.catchStep(program(environment))((error) => toStep(environment)(handler(error)));
  };
}
export function mapError($function) {
  return (program) => {
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const $value$1 = {
            tag: "Catch",
            _1: program(environment),
            _2: (caughtError) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument = /* @__PURE__ */ $function(caughtError);
                  const $value = {
                    tag: "Throw",
                    _1: asyncArgument
                  };
                  return $value;
                }
              };
            }
          };
          return $value$1;
        }
      };
    };
  };
}
export function attempt(program) {
  const composeOuter = /* @__PURE__ */ Control_Applicative.pure(applicativeAsync);
  return (environment) => {
    return {
      tag: "Catch",
      _1: {
        tag: "Run",
        _1: () => {
          const $value = {
            tag: "Bind",
            _1: program(environment),
            _2: (mappedValue) => ({
              tag: "Pure",
              _1: {
                tag: "Right",
                _1: mappedValue
              }
            })
          };
          return $value;
        }
      },
      _2: (caughtError) => {
        return {
          tag: "Run",
          _1: () => {
            const $value$1 = /* @__PURE__ */ composeOuter({
              tag: "Left",
              _1: caughtError
            })(environment);
            return $value$1;
          }
        };
      }
    };
  };
}
export function async(register) {
  return (environment) => {
    return {
      tag: "Async",
      _1: (callback) => {
        const $action = register(callback);
        return () => {
          const canceler = $action();
          const $value = canceler(environment);
          return $value;
        };
      }
    };
  };
}
export function fork(program) {
  return (environment) => Iris_Effect_Fiber.forkStep(program(environment))(/* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn));
}
export function join(fiber) {
  return ($record) => {
    return Iris_Effect_Fiber.bindStep(awaitStep(fiber))((section622) => {
      if (section622.tag === "Succeeded") {
        const { _1: value } = section622;
        return {
          tag: "Pure",
          _1: value
        };
      }
      if (section622.tag === "Failed") {
        const { _1: error } = section622;
        return {
          tag: "Throw",
          _1: error
        };
      }
      if (section622.tag === "Died") {
        const { _1: defect } = section622;
        return {
          tag: "Sync",
          _1: Effect_Exception.throwException(defect)
        };
      }
      throw new Error("Pattern match failure");
    });
  };
}
function $await(fiber) {
  return ($record) => awaitStep(fiber);
}
function awaitStep(fiber) {
  return {
    tag: "Async",
    _1: (callback) => {
      const $action = Iris_Effect_Fiber.joinFiber(fiber)((exit) => callback({
        tag: "Right",
        _1: exit
      }));
      return () => {
        const cancel = $action();
        const $value = {
          tag: "Sync",
          _1: cancel
        };
        return $value;
      };
    }
  };
}
export function kill(error) {
  return (fiber) => {
    return ($record) => {
      return {
        tag: "Async",
        _1: (callback) => {
          const $action = Iris_Effect_Fiber.killFiber(error)(fiber)(callback({
            tag: "Right",
            _1: Data_Unit.unit
          }));
          return () => {
            const cancel = $action();
            const $value = {
              tag: "Sync",
              _1: cancel
            };
            return $value;
          };
        }
      };
    };
  };
}
export function bracket(acquire) {
  return (release) => {
    return (use) => {
      return (environment) => {
        const releaseStep = (resource) => {
          return toStep(environment)(release(resource));
        };
        const $function = Iris_Effect_Fiber.bracketStep(toStep(environment)(acquire))({
          killed: releaseStep,
          failed: ($error) => releaseStep,
          died: ($error$1) => releaseStep,
          completed: ($value) => releaseStep
        });
        const composeOuter = toStep(environment);
        const $result = (composeArgument) => /* @__PURE__ */ composeOuter(/* @__PURE__ */ use(composeArgument));
        return $function($result);
      };
    };
  };
}
function $finally(release) {
  return (program) => {
    return bracket((environment) => ({
      tag: "Pure",
      _1: Data_Unit.unit
    }))(($unit) => release)(($unit$1) => program);
  };
}
export function launch($record) {
  const onUncaught = $record.onUncaught;
  return (environment) => {
    return (program) => {
      const $action = Iris_Effect_Fiber.newRuntime;
      return () => {
        const runtime = $action();
        const $action$1 = Iris_Effect_Fiber.makeFiber(runtime)(program(environment));
        const fiber = $action$1();
        const $action$2 = Iris_Effect_Fiber.joinFiber(fiber)((section1050) => {
          if (section1050.tag === "Succeeded") {
            const $value = Data_Unit.unit;
            return () => {
              return $value;
            };
          }
          if (section1050.tag === "Failed") {
            const { _1: error } = section1050;
            return onUncaught(error);
          }
          if (section1050.tag === "Died") {
            const { _1: defect } = section1050;
            return Effect_Exception.throwException(defect);
          }
          throw new Error("Pattern match failure");
        });
        const $effect = $action$2();
        return fiber;
      };
    };
  };
}
function deferred(environment) {
  return (program) => {
    return Iris_Effect_Fiber.deferStep(($unit) => program(environment));
  };
}
export const functorAsync = { map: ($function) => (program) => (environment) => Iris_Effect_Fiber.mapStep($function)(deferred(environment)(program)) };
export const applyAsync = {
  Functor0: () => functorAsync,
  apply: ($function) => (argument) => (environment) => Iris_Effect_Fiber.bindStep(deferred(environment)($function))((apply) => Iris_Effect_Fiber.mapStep(apply)(deferred(environment)(argument)))
};
export const applicativeAsync = {
  Apply0: () => applyAsync,
  pure: (value) => ($record) => ({
    tag: "Pure",
    _1: value
  })
};
export const bindAsync = {
  Apply0: () => applyAsync,
  bind: (program) => (continuation) => (environment) => Iris_Effect_Fiber.bindStep(deferred(environment)(program))((value) => toStep(environment)(continuation(value)))
};
export const monadAsync = {
  Applicative0: () => applicativeAsync,
  Bind1: () => bindAsync
};
export { $catch as "catch" };
export { $await as "await" };
export { $finally as "finally" };
