import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
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
function describe(section78) {
  if (section78.tag === "Left") {
    const { _1: error } = section78;
    return "left " + error;
  }
  if (section78.tag === "Right") {
    const { _1: value } = section78;
    return "right " + /* @__PURE__ */ showIntDictShow(value);
  }
  throw new Error("Pattern match failure");
}
function program(report) {
  const asyncArgument = say(report)("before fail");
  const asyncArgument$1 = say(report)("after fail");
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
                  _2: (firstValue) => ({
                    tag: "Throw",
                    _1: "boom"
                  })
                };
              }
            },
            _2: (caughtError) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$2 = say(report)("caught " + caughtError);
                  return {
                    tag: "Bind",
                    _1: asyncArgument$2(environment),
                    _2: ($unit) => ({
                      tag: "Pure",
                      _1: 7 | 0
                    })
                  };
                }
              };
            }
          },
          _2: (recovered) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$3 = say(report)("recovered " + /* @__PURE__ */ showIntDictShow(recovered));
                return {
                  tag: "Bind",
                  _1: asyncArgument$3(environment),
                  _2: ($unit$1) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        return {
                          tag: "Bind",
                          _1: {
                            tag: "Catch",
                            _1: {
                              tag: "Bind",
                              _1: {
                                tag: "Catch",
                                _1: {
                                  tag: "Throw",
                                  _1: "inner"
                                },
                                _2: (caughtError$1) => ({
                                  tag: "Throw",
                                  _1: "mapped " + caughtError$1
                                })
                              },
                              _2: (attemptValue) => ({
                                tag: "Pure",
                                _1: {
                                  tag: "Right",
                                  _1: attemptValue
                                }
                              })
                            },
                            _2: (caughtError$2) => ({
                              tag: "Pure",
                              _1: {
                                tag: "Left",
                                _1: caughtError$2
                              }
                            })
                          },
                          _2: (mapped) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$4 = say(report)(describe(mapped));
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$4(environment),
                                  _2: ($unit$2) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        return {
                                          tag: "Bind",
                                          _1: {
                                            tag: "Catch",
                                            _1: {
                                              tag: "Bind",
                                              _1: {
                                                tag: "Catch",
                                                _1: {
                                                  tag: "Throw",
                                                  _1: "first"
                                                },
                                                _2: (caughtError$3) => {
                                                  return {
                                                    tag: "Run",
                                                    _1: () => {
                                                      const asyncArgument$5 = "handled " + caughtError$3;
                                                      return {
                                                        tag: "Throw",
                                                        _1: asyncArgument$5
                                                      };
                                                    }
                                                  };
                                                }
                                              },
                                              _2: (attemptValue$1) => ({
                                                tag: "Pure",
                                                _1: {
                                                  tag: "Right",
                                                  _1: attemptValue$1
                                                }
                                              })
                                            },
                                            _2: (caughtError$4) => ({
                                              tag: "Pure",
                                              _1: {
                                                tag: "Left",
                                                _1: caughtError$4
                                              }
                                            })
                                          },
                                          _2: (rethrown) => {
                                            return {
                                              tag: "Run",
                                              _1: () => {
                                                const asyncArgument$6 = say(report)(describe(rethrown));
                                                return {
                                                  tag: "Bind",
                                                  _1: asyncArgument$6(environment),
                                                  _2: ($unit$3) => {
                                                    return {
                                                      tag: "Run",
                                                      _1: () => {
                                                        return {
                                                          tag: "Bind",
                                                          _1: {
                                                            tag: "Catch",
                                                            _1: {
                                                              tag: "Pure",
                                                              _1: {
                                                                tag: "Right",
                                                                _1: 3 | 0
                                                              }
                                                            },
                                                            _2: (caughtError$5) => ({
                                                              tag: "Pure",
                                                              _1: {
                                                                tag: "Left",
                                                                _1: caughtError$5
                                                              }
                                                            })
                                                          },
                                                          _2: (succeeded) => {
                                                            return {
                                                              tag: "Run",
                                                              _1: () => {
                                                                const asyncArgument$7 = say(report)(describe(succeeded));
                                                                return {
                                                                  tag: "Bind",
                                                                  _1: asyncArgument$7(environment),
                                                                  _2: ($unit$4) => {
                                                                    return {
                                                                      tag: "Run",
                                                                      _1: () => {
                                                                        return {
                                                                          tag: "Bind",
                                                                          _1: {
                                                                            tag: "Catch",
                                                                            _1: {
                                                                              tag: "Throw",
                                                                              _1: 42 | 0
                                                                            },
                                                                            _2: (caughtError$6) => ({
                                                                              tag: "Throw",
                                                                              _1: caughtError$6
                                                                            })
                                                                          },
                                                                          _2: ($unit$5) => say(report)("after uncaught")(environment)
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
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (code) => report("uncaught " + /* @__PURE__ */ showIntDictShow(code)) })({})(program(report)));
}
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
