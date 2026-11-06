import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Fiber from "../Iris.Effect.Fiber/index.js";
import * as $foreign from "./foreign.js";
function say(report) {
  return (message) => {
    const asyncArgument = report(message);
    return (environment) => {
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
  };
}
function killSelf(fiber) {
  const $function = Data_Functor.void(Effect.functorEffect);
  const $function$1 = Iris_Effect_Fiber.killFiber("self")(fiber);
  const $value = Data_Unit.unit;
  const asyncArgument = /* @__PURE__ */ $function($function$1(() => {
    return $value;
  }));
  return (environment) => {
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
}
function withSelf(report) {
  return (body) => {
    const $function = Data_Functor.void(Effect.functorEffect);
    const $function$1 = Iris_Effect_Async.launch({ onUncaught: (failure) => report("uncaught " + failure) })({});
    const asyncArgument = () => {
      return { value: "Nothing" };
    };
    const $result = (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const self = asyncArgument();
          const $function$2 = Iris_Effect_Async.fork;
          const asyncArgument$2 = Iris_Effect_Async.async((callback) => {
            const $action = later(callback({
              tag: "Right",
              _1: Data_Unit.unit
            }));
            return () => {
              const $unit = $action();
              return (environment$1) => ({
                tag: "Pure",
                _1: Data_Unit.unit
              });
            };
          });
          const $result$1 = (environment$2) => {
            return {
              tag: "Run",
              _1: () => {
                return {
                  tag: "Bind",
                  _1: asyncArgument$2(environment$2),
                  _2: ($unit$1) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$3 = () => {
                          return self.value;
                        };
                        const found = asyncArgument$3();
                        if (found.tag === "Just") {
                          const { _1: me } = found;
                          return body(me)(environment$2);
                        }
                        if (found === "Nothing") {
                          return say(report)("missing fiber")(environment$2);
                        }
                        throw new Error("Pattern match failure");
                      }
                    };
                  }
                };
              }
            };
          };
          const asyncArgument$1 = $function$2($result$1);
          return {
            tag: "Bind",
            _1: asyncArgument$1(environment),
            _2: (fiber) => {
              return {
                tag: "Run",
                _1: () => {
                  const $value = {
                    tag: "Just",
                    _1: fiber
                  };
                  const asyncArgument$4 = () => {
                    self.value = $value;
                    return undefined;
                  };
                  const $unit$2 = asyncArgument$4();
                  const asyncArgument$5 = Iris_Effect_Async.await(fiber);
                  return {
                    tag: "Bind",
                    _1: asyncArgument$5(environment),
                    _2: (exit) => {
                      const $function$3 = say(report);
                      let $result$2;
                      $case: {
                        if (exit.tag === "Succeeded") {
                          $result$2 = "succeeded";
                          break $case;
                        }
                        if (exit.tag === "Failed") {
                          const { _1: failure$1 } = exit;
                          $result$2 = "failed " + failure$1;
                          break $case;
                        }
                        if (exit.tag === "Died") {
                          $result$2 = "died";
                          break $case;
                        }
                        throw new Error("Pattern match failure");
                      }
                      return $function$3($result$2)(environment);
                    }
                  };
                }
              };
            }
          };
        }
      };
    };
    return /* @__PURE__ */ $function($function$1($result));
  };
}
export function insideCatch(report) {
  return withSelf(report)((me) => {
    const asyncArgument = say(report)("before kill");
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: {
              tag: "Catch",
              _1: {
                tag: "Run",
                _1: () => {
                  return {
                    tag: "Bind",
                    _1: asyncArgument(environment),
                    _2: ($unit) => {
                      return {
                        tag: "Run",
                        _1: () => {
                          const asyncArgument$1 = killSelf(me);
                          return {
                            tag: "Bind",
                            _1: asyncArgument$1(environment),
                            _2: ($unit$1) => say(report)("after kill")(environment)
                          };
                        }
                      };
                    }
                  };
                }
              },
              _2: (caughtError) => {
                return {
                  tag: "Run",
                  _1: () => {
                    return say(report)("caught " + caughtError)(environment);
                  }
                };
              }
            },
            _2: ($unit$2) => say(report)("after catch")(environment)
          };
        }
      };
    };
  });
}
export function beforeImmediateCallback(report) {
  return withSelf(report)((me) => {
    const asyncArgument = say(report)("before kill");
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: asyncArgument(environment),
            _2: ($unit) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$1 = killSelf(me);
                  return {
                    tag: "Bind",
                    _1: asyncArgument$1(environment),
                    _2: ($unit$1) => {
                      return {
                        tag: "Run",
                        _1: () => {
                          const asyncArgument$2 = Iris_Effect_Async.async((callback) => {
                            const $action = callback({
                              tag: "Right",
                              _1: 5 | 0
                            });
                            return () => {
                              const $unit$2 = $action();
                              return (environment$1) => ({
                                tag: "Pure",
                                _1: Data_Unit.unit
                              });
                            };
                          });
                          return {
                            tag: "Bind",
                            _1: asyncArgument$2(environment),
                            _2: (value) => say(report)("after callback " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(value))(environment)
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
  });
}
const later = $foreign["later"];
