import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Console from "../Iris.Effect.Console/index.js";
import * as Iris_Effect_Defaults from "../Iris.Effect.Defaults/index.js";
import * as Iris_Effect_Http from "../Iris.Effect.Http/index.js";
function welcome(services) {
  return (name) => {
    const syncArgument = /* @__PURE__ */ Iris_Effect_Console.log(services)("Welcome, " + name);
    return (environment) => {
      const $action = syncArgument(environment);
      return () => {
        const $unit = $action();
        return (/* @__PURE__ */ Iris_Effect_Console.error(services)("Audit: " + name)(environment))();
      };
    };
  };
}
function fetched(services) {
  return (url) => {
    const asyncArgument = /* @__PURE__ */ Iris_Effect_Http.get(services)(url);
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: asyncArgument(environment),
            _2: (response) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$1 = /* @__PURE__ */ Iris_Effect_Console.log(services)(/* @__PURE__ */ Data_Show.show(Data_Show.showInt)(response.status) + (" " + response.body));
                  const $action = asyncArgument$1(environment);
                  const value = $action();
                  return {
                    tag: "Pure",
                    _1: value
                  };
                }
              };
            }
          };
        }
      };
    };
  };
}
export function fetchGreeting(url) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(/* @__PURE__ */ Iris_Effect_Defaults.launchDefaults({})(/* @__PURE__ */ Iris_Effect_Defaults.defaultServicesCons({ reflectSymbol: ($proxy) => "console" })(Iris_Effect_Console.defaultServiceConsole)(/* @__PURE__ */ Iris_Effect_Defaults.defaultServicesCons({ reflectSymbol: ($proxy) => "http" })(Iris_Effect_Http.defaultServiceHttp)(Iris_Effect_Defaults.defaultServicesNil)({})({}))({})({}))({ onUncaught: (error) => /* @__PURE__ */ Iris_Effect_Defaults.runDefaults({})(/* @__PURE__ */ Iris_Effect_Defaults.defaultServicesCons({ reflectSymbol: ($proxy) => "console" })(Iris_Effect_Console.defaultServiceConsole)(Iris_Effect_Defaults.defaultServicesNil)({})({}))(/* @__PURE__ */ Iris_Effect_Console.error({ "Iris.Effect.Console.Console": (environment) => environment.console })(/* @__PURE__ */ Data_Show.show(Iris_Effect_Http.showHttpError)(error))) })(/* @__PURE__ */ fetched({
    "Iris.Effect.Console.Console": (environment$1) => environment$1.console,
    "Iris.Effect.Http.Http": (environment$2) => environment$2.http
  })(url)));
}
export const greet = /* @__PURE__ */ (() => {
  const $function = Iris_Effect_Defaults.runDefaults({})(/* @__PURE__ */ Iris_Effect_Defaults.defaultServicesCons({ reflectSymbol: ($proxy) => "console" })(Iris_Effect_Console.defaultServiceConsole)(Iris_Effect_Defaults.defaultServicesNil)({})({}));
  const syncArgument = /* @__PURE__ */ welcome({ "Iris.Effect.Console.Console": (environment) => environment.console })("Alice");
  const $result = (environment$1) => {
    const $action = syncArgument(environment$1);
    return () => {
      const $unit = $action();
      return (/* @__PURE__ */ welcome({ "Iris.Effect.Console.Console": (environment$2) => environment$2.console })("Bob")(environment$1))();
    };
  };
  return /* @__PURE__ */ $function($result);
})();
