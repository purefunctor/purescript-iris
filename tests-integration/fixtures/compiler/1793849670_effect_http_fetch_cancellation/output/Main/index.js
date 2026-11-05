import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Defaults from "../Iris.Effect.Defaults/index.js";
import * as Iris_Effect_Http from "../Iris.Effect.Http/index.js";
function arrival(options) {
  return Iris_Effect_Async.async((callback) => {
    const $action = options.onArrived(callback({
      tag: "Right",
      _1: Data_Unit.unit
    }));
    return () => {
      const $unit = $action();
      return (environment) => ({
        tag: "Pure",
        _1: Data_Unit.unit
      });
    };
  });
}
export function cancelFetch(options) {
  const $function = Data_Functor.void(Effect.functorEffect);
  const $function$1 = Iris_Effect_Defaults.launchDefaults({})(/* @__PURE__ */ Iris_Effect_Defaults.defaultServicesCons({ reflectSymbol: ($proxy) => "http" })(Iris_Effect_Http.defaultServiceHttp)(Iris_Effect_Defaults.defaultServicesNil)({})({}));
  const composeOuter = options.report;
  const $result = (composeArgument) => /* @__PURE__ */ composeOuter(/* @__PURE__ */ showHttpErrorDictShow(composeArgument));
  const $function$2 = $function$1({ onUncaught: $result });
  const asyncArgument = Iris_Effect_Async.fork(/* @__PURE__ */ Iris_Effect_Http.get({ "Iris.Effect.Http.Http": (environment) => environment.http })(options.url));
  const $result$1 = (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument(environment$1),
          _2: (pending) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$1 = arrival(options);
                return {
                  tag: "Bind",
                  _1: asyncArgument$1(environment$1),
                  _2: ($unit) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$2 = Iris_Effect_Async.kill({
                          tag: "NetworkError",
                          _1: "cancelled"
                        })(pending);
                        return {
                          tag: "Bind",
                          _1: asyncArgument$2(environment$1),
                          _2: ($unit$1) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$3 = Iris_Effect_Async.await(pending);
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$3(environment$1),
                                  _2: (exit) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const $function$3 = options.report;
                                        let $result$2;
                                        $case: {
                                          if (exit.tag === "Succeeded") {
                                            const { _1: response } = exit;
                                            $result$2 = "succeeded " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(response.status);
                                            break $case;
                                          }
                                          if (exit.tag === "Failed") {
                                            const { _1: error } = exit;
                                            $result$2 = "failed " + /* @__PURE__ */ showHttpErrorDictShow(error);
                                            break $case;
                                          }
                                          if (exit.tag === "Died") {
                                            $result$2 = "died";
                                            break $case;
                                          }
                                          throw new Error("Pattern match failure");
                                        }
                                        const asyncArgument$4 = $function$3($result$2);
                                        const value = asyncArgument$4();
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
                          }
                        };
                      }
                    };
                  }
                };
              }
            };
          }
        };
      }
    };
  };
  return /* @__PURE__ */ $function(/* @__PURE__ */ $function$2($result$1));
}
const showHttpErrorDictShow = /* @__PURE__ */ Data_Show.show(Iris_Effect_Http.showHttpError);
