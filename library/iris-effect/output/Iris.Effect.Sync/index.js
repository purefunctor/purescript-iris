import * as Iris_Effect from "../Iris.Effect/index.js";
export function make(value) {
  return value;
}
export function run(environment) {
  return (program) => {
    return program(environment);
  };
}
export function service(hasServiceServicesDict) {
  const $closure = (environment) => {
    const $value = /* @__PURE__ */ Iris_Effect.getService(hasServiceServicesDict)(environment);
    return () => {
      return $value;
    };
  };
  return $closure;
}
export function liftEffect(effect) {
  return ($record) => effect;
}
export function provide(environment) {
  return (program) => {
    return ($record) => program(environment);
  };
}
export const functorSync = /* @__PURE__ */ (() => {
  const $closure = ($function) => {
    return (program) => {
      const $closure$1 = (environment) => {
        const $action = program(environment);
        return () => {
          return $function($action());
        };
      };
      return $closure$1;
    };
  };
  return { map: $closure };
})();
export const applySync = /* @__PURE__ */ (() => {
  const $closure = ($function) => {
    return (argument) => {
      const $closure$1 = (environment) => {
        const $functionAction = $function(environment);
        const $argumentAction = argument(environment);
        return () => {
          return $functionAction()($argumentAction());
        };
      };
      return $closure$1;
    };
  };
  return {
    Functor0: () => functorSync,
    apply: $closure
  };
})();
export const applicativeSync = /* @__PURE__ */ (() => {
  const $closure = (value) => {
    const $closure$1 = ($record) => {
      return () => {
        return value;
      };
    };
    return $closure$1;
  };
  return {
    Apply0: () => applySync,
    pure: $closure
  };
})();
export const bindSync = /* @__PURE__ */ (() => {
  const $closure = (program) => {
    return (continuation) => {
      const $closure$1 = (environment) => {
        const $action = program(environment);
        return () => {
          const value = $action();
          return run(environment)(continuation(value))();
        };
      };
      return $closure$1;
    };
  };
  return {
    Apply0: () => applySync,
    bind: $closure
  };
})();
export const monadSync = {
  Applicative0: () => applicativeSync,
  Bind1: () => bindSync
};
