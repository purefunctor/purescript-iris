export function make(value) {
  return value;
}
export function run(environment) {
  return (program) => {
    return program(environment);
  };
}
export function service(hasServiceServicesDict) {
  return (environment) => {
    const $value = hasServiceServicesDict.getService(environment);
    return () => {
      return $value;
    };
  };
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
  return { map: ($function) => {
    return (program) => {
      return (environment) => {
        const $action = program(environment);
        return () => {
          return $function($action());
        };
      };
    };
  } };
})();
export const applySync = /* @__PURE__ */ (() => {
  return {
    Functor0: () => functorSync,
    apply: ($function) => {
      return (argument) => {
        return (environment) => {
          const $functionAction = $function(environment);
          const $argumentAction = argument(environment);
          return () => {
            return $functionAction()($argumentAction());
          };
        };
      };
    }
  };
})();
export const applicativeSync = /* @__PURE__ */ (() => {
  return {
    Apply0: () => applySync,
    pure: (value) => {
      return ($record) => {
        return () => {
          return value;
        };
      };
    }
  };
})();
export const bindSync = /* @__PURE__ */ (() => {
  return {
    Apply0: () => applySync,
    bind: (program) => {
      return (continuation) => {
        return (environment) => {
          const $action = program(environment);
          return () => {
            const value = $action();
            return continuation(value)(environment)();
          };
        };
      };
    }
  };
})();
export const monadSync = {
  Applicative0: () => applicativeSync,
  Bind1: () => bindSync
};
