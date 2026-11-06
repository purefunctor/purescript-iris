import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Effect_Exception from "../Effect.Exception/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as $foreign from "./foreign.js";
function delay(milliseconds) {
  return Iris_Effect_Async.async((callback) => {
    const $action = setTimeout(milliseconds)(callback({
      tag: "Right",
      _1: Data_Unit.unit
    }));
    return () => {
      const identifier = $action();
      const asyncArgument = clearTimeout(identifier);
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
  });
}
function describe(section144) {
  if (section144.tag === "Succeeded") {
    return "succeeded";
  }
  if (section144.tag === "Failed") {
    const { _1: error } = section144;
    return "failed " + error;
  }
  if (section144.tag === "Died") {
    const { _1: defect } = section144;
    return "died " + Effect_Exception.message(defect);
  }
  throw new Error("Pattern match failure");
}
function outcome(report) {
  return (label) => {
    return (program$1) => {
      const asyncArgument = Iris_Effect_Async.fork(program$1);
      return (environment) => {
        return {
          tag: "Run",
          _1: () => {
            return {
              tag: "Bind",
              _1: asyncArgument(environment),
              _2: (fiber) => {
                return {
                  tag: "Run",
                  _1: () => {
                    const asyncArgument$1 = Iris_Effect_Async.await(fiber);
                    return {
                      tag: "Bind",
                      _1: asyncArgument$1(environment),
                      _2: (exit) => {
                        return {
                          tag: "Run",
                          _1: () => {
                            const asyncArgument$2 = report(label + (": " + describe(exit)));
                            const value = asyncArgument$2();
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
      };
    };
  };
}
function program(report) {
  const say = (message) => {
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
  const $function = Iris_Effect_Async.fork;
  const $function$1 = Iris_Effect_Async.finally(say("released"));
  const asyncArgument$2 = boom("boom");
  const $result = (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        const value$1 = asyncArgument$2();
        return {
          tag: "Pure",
          _1: value$1
        };
      }
    };
  };
  const asyncArgument$1 = $function($function$1($result));
  return (environment$2) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument$1(environment$2),
          _2: (dying) => {
            return {
              tag: "Run",
              _1: () => {
                const $function$2 = Iris_Effect_Async.fork;
                const asyncArgument$4 = delay(10 | 0);
                const asyncArgument$5 = say("sibling completed");
                const $result$1 = (environment$3) => {
                  return {
                    tag: "Run",
                    _1: () => {
                      return {
                        tag: "Bind",
                        _1: asyncArgument$4(environment$3),
                        _2: (firstValue) => ({
                          tag: "Bind",
                          _1: asyncArgument$5(environment$3),
                          _2: (secondValue) => ({
                            tag: "Pure",
                            _1: secondValue
                          })
                        })
                      };
                    }
                  };
                };
                const asyncArgument$3 = $function$2($result$1);
                return {
                  tag: "Bind",
                  _1: asyncArgument$3(environment$2),
                  _2: (other) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$6 = Iris_Effect_Async.await(other);
                        return {
                          tag: "Bind",
                          _1: asyncArgument$6(environment$2),
                          _2: ($exit) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$7 = outcome(report)("await")(Iris_Effect_Async.join(dying));
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$7(environment$2),
                                  _2: ($unit) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const $function$3 = outcome(report)("attempt");
                                        const asyncArgument$9 = Iris_Effect_Async.join(dying);
                                        const $result$2 = (environment$4) => {
                                          return {
                                            tag: "Run",
                                            _1: () => {
                                              return {
                                                tag: "Catch",
                                                _1: {
                                                  tag: "Bind",
                                                  _1: asyncArgument$9(environment$4),
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
                                        const asyncArgument$8 = $function$3($result$2);
                                        return {
                                          tag: "Bind",
                                          _1: asyncArgument$8(environment$2),
                                          _2: ($unit$1) => {
                                            return {
                                              tag: "Run",
                                              _1: () => {
                                                const $function$4 = outcome(report)("cleanup over typed");
                                                const $function$5 = Iris_Effect_Async.finally;
                                                const asyncArgument$11 = boom("cleanup");
                                                const $result$3 = (environment$5) => {
                                                  return {
                                                    tag: "Run",
                                                    _1: () => {
                                                      const value$2 = asyncArgument$11();
                                                      return {
                                                        tag: "Pure",
                                                        _1: value$2
                                                      };
                                                    }
                                                  };
                                                };
                                                const asyncArgument$10 = $function$4($function$5($result$3)((environment$6) => ({
                                                  tag: "Throw",
                                                  _1: "typed"
                                                })));
                                                return {
                                                  tag: "Bind",
                                                  _1: asyncArgument$10(environment$2),
                                                  _2: ($unit$2) => {
                                                    return {
                                                      tag: "Run",
                                                      _1: () => {
                                                        const $function$6 = outcome(report)("body over cleanup");
                                                        const $function$7 = Iris_Effect_Async.finally;
                                                        const asyncArgument$13 = boom("cleanup");
                                                        const $result$4 = (environment$7) => {
                                                          return {
                                                            tag: "Run",
                                                            _1: () => {
                                                              const value$3 = asyncArgument$13();
                                                              return {
                                                                tag: "Pure",
                                                                _1: value$3
                                                              };
                                                            }
                                                          };
                                                        };
                                                        const $function$8 = $function$7($result$4);
                                                        const asyncArgument$14 = boom("body");
                                                        const $result$5 = (environment$8) => {
                                                          return {
                                                            tag: "Run",
                                                            _1: () => {
                                                              const value$4 = asyncArgument$14();
                                                              return {
                                                                tag: "Pure",
                                                                _1: value$4
                                                              };
                                                            }
                                                          };
                                                        };
                                                        const asyncArgument$12 = $function$6($function$8($result$5));
                                                        return {
                                                          tag: "Bind",
                                                          _1: asyncArgument$12(environment$2),
                                                          _2: ($unit$3) => {
                                                            return {
                                                              tag: "Run",
                                                              _1: () => {
                                                                const $function$9 = Iris_Effect_Async.fork;
                                                                const $function$10 = Iris_Effect_Async.finally;
                                                                const asyncArgument$16 = boom("finalizer");
                                                                const $result$6 = (environment$9) => {
                                                                  return {
                                                                    tag: "Run",
                                                                    _1: () => {
                                                                      const value$5 = asyncArgument$16();
                                                                      return {
                                                                        tag: "Pure",
                                                                        _1: value$5
                                                                      };
                                                                    }
                                                                  };
                                                                };
                                                                const asyncArgument$15 = $function$9($function$10($result$6)(delay(1e3 | 0)));
                                                                return {
                                                                  tag: "Bind",
                                                                  _1: asyncArgument$15(environment$2),
                                                                  _2: (killed) => {
                                                                    return {
                                                                      tag: "Run",
                                                                      _1: () => {
                                                                        const asyncArgument$17 = Iris_Effect_Async.kill("killed")(killed);
                                                                        return {
                                                                          tag: "Bind",
                                                                          _1: asyncArgument$17(environment$2),
                                                                          _2: ($unit$4) => {
                                                                            return {
                                                                              tag: "Run",
                                                                              _1: () => {
                                                                                const asyncArgument$18 = Iris_Effect_Async.await(killed);
                                                                                return {
                                                                                  tag: "Bind",
                                                                                  _1: asyncArgument$18(environment$2),
                                                                                  _2: (killedExit) => {
                                                                                    return {
                                                                                      tag: "Run",
                                                                                      _1: () => {
                                                                                        const asyncArgument$19 = say("kill with failing finalizer: " + describe(killedExit));
                                                                                        return {
                                                                                          tag: "Bind",
                                                                                          _1: asyncArgument$19(environment$2),
                                                                                          _2: ($unit$5) => {
                                                                                            return {
                                                                                              tag: "Run",
                                                                                              _1: () => {
                                                                                                const asyncArgument$20 = outcome(report)("null")((environment$10) => {
                                                                                                  return {
                                                                                                    tag: "Run",
                                                                                                    _1: () => {
                                                                                                      const value$6 = throwNull();
                                                                                                      return {
                                                                                                        tag: "Pure",
                                                                                                        _1: value$6
                                                                                                      };
                                                                                                    }
                                                                                                  };
                                                                                                });
                                                                                                return {
                                                                                                  tag: "Bind",
                                                                                                  _1: asyncArgument$20(environment$2),
                                                                                                  _2: ($unit$6) => {
                                                                                                    return {
                                                                                                      tag: "Run",
                                                                                                      _1: () => {
                                                                                                        const asyncArgument$21 = outcome(report)("hostile")((environment$11) => {
                                                                                                          return {
                                                                                                            tag: "Run",
                                                                                                            _1: () => {
                                                                                                              const value$7 = throwHostile();
                                                                                                              return {
                                                                                                                tag: "Pure",
                                                                                                                _1: value$7
                                                                                                              };
                                                                                                            }
                                                                                                          };
                                                                                                        });
                                                                                                        return {
                                                                                                          tag: "Bind",
                                                                                                          _1: asyncArgument$21(environment$2),
                                                                                                          _2: ($unit$7) => say("parent continues")(environment$2)
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
          }
        };
      }
    };
  };
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
export function launchDefect(report) {
  const $function = Data_Functor.void(Effect.functorEffect);
  const $function$1 = Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({});
  const asyncArgument = boom("root");
  const $result = (environment) => {
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
  return /* @__PURE__ */ $function($function$1($result));
}
const boom = $foreign["boom"];
const throwNull = $foreign["throwNull"];
const throwHostile = $foreign["throwHostile"];
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
