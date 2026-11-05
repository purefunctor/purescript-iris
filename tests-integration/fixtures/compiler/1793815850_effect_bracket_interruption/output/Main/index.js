import * as Data_Functor from "../Data.Functor/index.js";
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
          const $value = {
            tag: "Pure",
            _1: value
          };
          return $value;
        }
      };
    };
  };
}
function sleep(report) {
  return (label) => {
    return (milliseconds) => {
      return Iris_Effect_Async.async((callback) => {
        const $action = setTimeout(milliseconds)(callback({
          tag: "Right",
          _1: Data_Unit.unit
        }));
        return () => {
          const identifier = $action();
          const asyncArgument = say(report)("cancel " + label);
          const asyncArgument$1 = clearTimeout(identifier);
          const $result = (environment) => {
            return {
              tag: "Run",
              _1: () => {
                const $value$1 = {
                  tag: "Bind",
                  _1: asyncArgument(environment),
                  _2: (firstValue) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const secondValue = asyncArgument$1();
                        const $value = {
                          tag: "Pure",
                          _1: secondValue
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
          return $result;
        };
      });
    };
  };
}
function describe(section167) {
  if (section167.tag === "Succeeded") {
    return "succeeded";
  }
  if (section167.tag === "Failed") {
    const { _1: error } = section167;
    return "failed " + error;
  }
  if (section167.tag === "Died") {
    return "died";
  }
  throw new Error("Pattern match failure");
}
function killed(report) {
  return (label) => {
    return (body) => {
      const asyncArgument = Iris_Effect_Async.fork(body);
      return (environment) => {
        return {
          tag: "Run",
          _1: () => {
            const $value$2 = {
              tag: "Bind",
              _1: asyncArgument(environment),
              _2: (fiber) => {
                return {
                  tag: "Run",
                  _1: () => {
                    const asyncArgument$1 = Iris_Effect_Async.kill("kill " + label)(fiber);
                    const $value$1 = {
                      tag: "Bind",
                      _1: asyncArgument$1(environment),
                      _2: ($unit) => {
                        return {
                          tag: "Run",
                          _1: () => {
                            const asyncArgument$2 = Iris_Effect_Async.await(fiber);
                            const $value = {
                              tag: "Bind",
                              _1: asyncArgument$2(environment),
                              _2: (exit) => say(report)(label + (" " + describe(exit)))(environment)
                            };
                            return $value;
                          }
                        };
                      }
                    };
                    return $value$1;
                  }
                };
              }
            };
            return $value$2;
          }
        };
      };
    };
  };
}
function program(report) {
  const $function = killed(report)("during acquire");
  const $function$1 = Iris_Effect_Async.bracket;
  const asyncArgument$1 = sleep(report)("acquire")(10 | 0);
  const asyncArgument$2 = say(report)("acquired");
  const $result = (environment) => {
    return {
      tag: "Run",
      _1: () => {
        const $value = {
          tag: "Bind",
          _1: asyncArgument$1(environment),
          _2: (firstValue) => ({
            tag: "Bind",
            _1: asyncArgument$2(environment),
            _2: (secondValue) => ({
              tag: "Pure",
              _1: secondValue
            })
          })
        };
        return $value;
      }
    };
  };
  const asyncArgument = $function($function$1($result)(($unit) => say(report)("released"))(($unit$1) => say(report)("used")));
  return (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        const $value$4 = {
          tag: "Bind",
          _1: asyncArgument(environment$1),
          _2: ($unit$2) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$3 = killed(report)("during use")(Iris_Effect_Async.bracket(say(report)("acquired"))(($unit$3) => {
                  const asyncArgument$4 = sleep(report)("release")(10 | 0);
                  const asyncArgument$5 = say(report)("released");
                  return (environment$2) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const $value$1 = {
                          tag: "Bind",
                          _1: asyncArgument$4(environment$2),
                          _2: (firstValue$1) => ({
                            tag: "Bind",
                            _1: asyncArgument$5(environment$2),
                            _2: (secondValue$1) => ({
                              tag: "Pure",
                              _1: secondValue$1
                            })
                          })
                        };
                        return $value$1;
                      }
                    };
                  };
                })(($unit$4) => {
                  const asyncArgument$6 = sleep(report)("use")(1e3 | 0);
                  const asyncArgument$7 = say(report)("used");
                  return (environment$3) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const $value$2 = {
                          tag: "Bind",
                          _1: asyncArgument$6(environment$3),
                          _2: (firstValue$2) => ({
                            tag: "Bind",
                            _1: asyncArgument$7(environment$3),
                            _2: (secondValue$2) => ({
                              tag: "Pure",
                              _1: secondValue$2
                            })
                          })
                        };
                        return $value$2;
                      }
                    };
                  };
                }));
                const $value$3 = {
                  tag: "Bind",
                  _1: asyncArgument$3(environment$1),
                  _2: ($unit$5) => killed(report)("with failing release")(Iris_Effect_Async.bracket((environment$4) => ({
                    tag: "Pure",
                    _1: Data_Unit.unit
                  }))(($unit$6) => (environment$5) => ({
                    tag: "Throw",
                    _1: "release failure"
                  }))(($unit$7) => sleep(report)("use")(1e3 | 0)))(environment$1)
                };
                return $value$3;
              }
            };
          }
        };
        return $value$4;
      }
    };
  };
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
