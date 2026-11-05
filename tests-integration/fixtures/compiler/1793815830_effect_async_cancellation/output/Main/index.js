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
          return {
            tag: "Pure",
            _1: value
          };
        }
      };
    };
  };
}
function sleep(report) {
  return (milliseconds) => {
    return Iris_Effect_Async.async((callback) => {
      const $action = setTimeout(milliseconds)(callback({
        tag: "Right",
        _1: Data_Unit.unit
      }));
      return () => {
        const identifier = $action();
        const asyncArgument = say(report)("canceler");
        const asyncArgument$1 = clearTimeout(identifier);
        return (environment) => {
          return {
            tag: "Run",
            _1: () => {
              return {
                tag: "Bind",
                _1: asyncArgument(environment),
                _2: (firstValue) => {
                  return {
                    tag: "Run",
                    _1: () => {
                      const secondValue = asyncArgument$1();
                      return {
                        tag: "Pure",
                        _1: secondValue
                      };
                    }
                  };
                }
              };
            }
          };
        };
      };
    });
  };
}
function describe(section159) {
  if (section159.tag === "Succeeded") {
    return "succeeded";
  }
  if (section159.tag === "Failed") {
    const { _1: error } = section159;
    return "failed " + error;
  }
  if (section159.tag === "Died") {
    return "died";
  }
  throw new Error("Pattern match failure");
}
function program(report) {
  const $function = Iris_Effect_Async.fork;
  const asyncArgument$1 = sleep(report)(1e3 | 0);
  const asyncArgument$2 = say(report)("woke");
  const $result = (environment) => {
    return {
      tag: "Run",
      _1: () => {
        return {
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
      }
    };
  };
  const asyncArgument = $function($result);
  return (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument(environment$1),
          _2: (sleeping) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$3 = say(report)("forked");
                return {
                  tag: "Bind",
                  _1: asyncArgument$3(environment$1),
                  _2: ($unit) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$4 = Iris_Effect_Async.kill("stop")(sleeping);
                        return {
                          tag: "Bind",
                          _1: asyncArgument$4(environment$1),
                          _2: ($unit$1) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$5 = say(report)("killed");
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$5(environment$1),
                                  _2: ($unit$2) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const asyncArgument$6 = Iris_Effect_Async.await(sleeping);
                                        return {
                                          tag: "Bind",
                                          _1: asyncArgument$6(environment$1),
                                          _2: (sleepingExit) => {
                                            return {
                                              tag: "Run",
                                              _1: () => {
                                                const asyncArgument$7 = say(report)("sleeping " + describe(sleepingExit));
                                                return {
                                                  tag: "Bind",
                                                  _1: asyncArgument$7(environment$1),
                                                  _2: ($unit$3) => {
                                                    return {
                                                      tag: "Run",
                                                      _1: () => {
                                                        const asyncArgument$8 = Iris_Effect_Async.fork((environment$2) => ({
                                                          tag: "Pure",
                                                          _1: Data_Unit.unit
                                                        }));
                                                        return {
                                                          tag: "Bind",
                                                          _1: asyncArgument$8(environment$1),
                                                          _2: (finished) => {
                                                            return {
                                                              tag: "Run",
                                                              _1: () => {
                                                                const asyncArgument$9 = Iris_Effect_Async.await(finished);
                                                                return {
                                                                  tag: "Bind",
                                                                  _1: asyncArgument$9(environment$1),
                                                                  _2: ($exit) => {
                                                                    return {
                                                                      tag: "Run",
                                                                      _1: () => {
                                                                        const asyncArgument$10 = Iris_Effect_Async.kill("too late")(finished);
                                                                        return {
                                                                          tag: "Bind",
                                                                          _1: asyncArgument$10(environment$1),
                                                                          _2: ($unit$4) => {
                                                                            return {
                                                                              tag: "Run",
                                                                              _1: () => {
                                                                                const asyncArgument$11 = Iris_Effect_Async.await(finished);
                                                                                return {
                                                                                  tag: "Bind",
                                                                                  _1: asyncArgument$11(environment$1),
                                                                                  _2: (finishedExit) => {
                                                                                    return {
                                                                                      tag: "Run",
                                                                                      _1: () => {
                                                                                        const asyncArgument$12 = say(report)("finished " + describe(finishedExit));
                                                                                        return {
                                                                                          tag: "Bind",
                                                                                          _1: asyncArgument$12(environment$1),
                                                                                          _2: ($unit$5) => {
                                                                                            return {
                                                                                              tag: "Run",
                                                                                              _1: () => {
                                                                                                const asyncArgument$13 = sleep(report)(20 | 0);
                                                                                                return {
                                                                                                  tag: "Bind",
                                                                                                  _1: asyncArgument$13(environment$1),
                                                                                                  _2: ($unit$6) => say(report)("done")(environment$1)
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
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
