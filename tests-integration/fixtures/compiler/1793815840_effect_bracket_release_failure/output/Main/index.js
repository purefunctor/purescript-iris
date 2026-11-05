import * as Data_Either from "../Data.Either/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
function describe(section48) {
  if (section48.tag === "Left") {
    const { _1: error } = section48;
    return "failed " + error;
  }
  if (section48.tag === "Right") {
    const { _1: value } = section48;
    return "succeeded " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(value);
  }
  throw new Error("Pattern match failure");
}
function guarded(release) {
  return (body) => {
    const asyncArgument = Iris_Effect_Async.bracket((environment) => ({
      tag: "Pure",
      _1: Data_Unit.unit
    }))(($unit) => release)(($unit$1) => body);
    return (environment$1) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Catch",
            _1: {
              tag: "Bind",
              _1: asyncArgument(environment$1),
              _2: (attemptValue) => ({
                tag: "Pure",
                _1: {
                  tag: "Right",
                  _1: attemptValue
                }
              })
            },
            _2: (caughtError) => ({
              tag: "Pure",
              _1: {
                tag: "Left",
                _1: caughtError
              }
            })
          };
        }
      };
    };
  };
}
function program(report) {
  const report_ = (message) => {
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
  const asyncArgument$1 = guarded((environment$1) => ({
    tag: "Throw",
    _1: "release"
  }))((environment$2) => ({
    tag: "Pure",
    _1: 1 | 0
  }));
  return (environment$3) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument$1(environment$3),
          _2: (releaseFailed) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$2 = report_("release fails: " + describe(releaseFailed));
                return {
                  tag: "Bind",
                  _1: asyncArgument$2(environment$3),
                  _2: ($unit) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$3 = guarded((environment$4) => ({
                          tag: "Pure",
                          _1: Data_Unit.unit
                        }))((environment$5) => ({
                          tag: "Throw",
                          _1: "body"
                        }));
                        return {
                          tag: "Bind",
                          _1: asyncArgument$3(environment$3),
                          _2: (bodyFailed) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$4 = report_("body fails: " + describe(bodyFailed));
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$4(environment$3),
                                  _2: ($unit$1) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const asyncArgument$5 = guarded((environment$6) => ({
                                          tag: "Throw",
                                          _1: "release"
                                        }))((environment$7) => ({
                                          tag: "Throw",
                                          _1: "body"
                                        }));
                                        return {
                                          tag: "Bind",
                                          _1: asyncArgument$5(environment$3),
                                          _2: (bothFailed) => {
                                            return {
                                              tag: "Run",
                                              _1: () => {
                                                const asyncArgument$6 = report_("both fail: " + describe(bothFailed));
                                                return {
                                                  tag: "Bind",
                                                  _1: asyncArgument$6(environment$3),
                                                  _2: ($unit$2) => {
                                                    return {
                                                      tag: "Run",
                                                      _1: () => {
                                                        const asyncArgument$7 = guarded((environment$8) => ({
                                                          tag: "Pure",
                                                          _1: Data_Unit.unit
                                                        }))((environment$9) => ({
                                                          tag: "Pure",
                                                          _1: 2 | 0
                                                        }));
                                                        return {
                                                          tag: "Bind",
                                                          _1: asyncArgument$7(environment$3),
                                                          _2: (neitherFailed) => report_("neither fails: " + describe(neitherFailed))(environment$3)
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
