import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
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
function describe(section121) {
  if (section121.tag === "Left") {
    const { _1: error } = section121;
    return "failed " + error;
  }
  if (section121.tag === "Right") {
    const { _1: value } = section121;
    return "succeeded " + /* @__PURE__ */ showIntDictShow(value);
  }
  throw new Error("Pattern match failure");
}
function describeOuter(section160) {
  if (section160.tag === "Left") {
    const { _1: code } = section160;
    return "failed outer " + /* @__PURE__ */ showIntDictShow(code);
  }
  if (section160.tag === "Right") {
    const { _1: value } = section160;
    return "succeeded " + /* @__PURE__ */ showIntDictShow(value);
  }
  throw new Error("Pattern match failure");
}
function children(report) {
  const asyncArgument = Iris_Effect_Async.fork((environment) => ({
    tag: "Throw",
    _1: "detached child failure"
  }));
  return (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument(environment$1),
          _2: ($fiber) => {
            return {
              tag: "Run",
              _1: () => {
                const $function = Iris_Effect_Async.fork;
                const asyncArgument$2 = delay(10 | 0);
                const $result = (environment$2) => {
                  return {
                    tag: "Run",
                    _1: () => {
                      return {
                        tag: "Bind",
                        _1: asyncArgument$2(environment$2),
                        _2: (firstValue) => ({
                          tag: "Throw",
                          _1: "joined child failure"
                        })
                      };
                    }
                  };
                };
                const asyncArgument$1 = $function($result);
                return {
                  tag: "Bind",
                  _1: asyncArgument$1(environment$1),
                  _2: (failing) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const $function$1 = Iris_Effect_Async.fork;
                        const asyncArgument$4 = delay(10 | 0);
                        const $result$1 = (environment$3) => {
                          return {
                            tag: "Run",
                            _1: () => {
                              return {
                                tag: "Bind",
                                _1: asyncArgument$4(environment$3),
                                _2: (firstValue$1) => {
                                  return {
                                    tag: "Run",
                                    _1: () => {
                                      return {
                                        tag: "Pure",
                                        _1: 5 | 0
                                      };
                                    }
                                  };
                                }
                              };
                            }
                          };
                        };
                        const asyncArgument$3 = $function$1($result$1);
                        return {
                          tag: "Bind",
                          _1: asyncArgument$3(environment$1),
                          _2: (succeeding) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$5 = Iris_Effect_Async.fork(innerRelease);
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$5(environment$1),
                                  _2: (guarded) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const asyncArgument$6 = delay(20 | 0);
                                        return {
                                          tag: "Bind",
                                          _1: asyncArgument$6(environment$1),
                                          _2: ($unit) => {
                                            return {
                                              tag: "Run",
                                              _1: () => {
                                                const asyncArgument$7 = Iris_Effect_Async.kill("killed")(guarded);
                                                return {
                                                  tag: "Bind",
                                                  _1: asyncArgument$7(environment$1),
                                                  _2: ($unit$1) => {
                                                    return {
                                                      tag: "Run",
                                                      _1: () => {
                                                        const asyncArgument$8 = Iris_Effect_Async.join(failing);
                                                        return {
                                                          tag: "Bind",
                                                          _1: {
                                                            tag: "Catch",
                                                            _1: {
                                                              tag: "Bind",
                                                              _1: asyncArgument$8(environment$1),
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
                                                          },
                                                          _2: (joinedFailing) => {
                                                            return {
                                                              tag: "Run",
                                                              _1: () => {
                                                                const asyncArgument$9 = Iris_Effect_Async.join(succeeding);
                                                                return {
                                                                  tag: "Bind",
                                                                  _1: {
                                                                    tag: "Catch",
                                                                    _1: {
                                                                      tag: "Bind",
                                                                      _1: asyncArgument$9(environment$1),
                                                                      _2: (attemptValue$1) => ({
                                                                        tag: "Pure",
                                                                        _1: {
                                                                          tag: "Right",
                                                                          _1: attemptValue$1
                                                                        }
                                                                      })
                                                                    },
                                                                    _2: (caughtError$1) => ({
                                                                      tag: "Pure",
                                                                      _1: {
                                                                        tag: "Left",
                                                                        _1: caughtError$1
                                                                      }
                                                                    })
                                                                  },
                                                                  _2: (joinedSucceeding) => {
                                                                    return {
                                                                      tag: "Run",
                                                                      _1: () => {
                                                                        const asyncArgument$10 = Iris_Effect_Async.join(guarded);
                                                                        return {
                                                                          tag: "Bind",
                                                                          _1: {
                                                                            tag: "Catch",
                                                                            _1: {
                                                                              tag: "Bind",
                                                                              _1: asyncArgument$10(environment$1),
                                                                              _2: (attemptValue$2) => ({
                                                                                tag: "Pure",
                                                                                _1: {
                                                                                  tag: "Right",
                                                                                  _1: attemptValue$2
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
                                                                          _2: (joinedGuarded) => {
                                                                            return {
                                                                              tag: "Run",
                                                                              _1: () => {
                                                                                const asyncArgument$11 = report(describe(joinedFailing) + (", " + (describe(joinedSucceeding) + (", " + describe(joinedGuarded)))));
                                                                                const $unit$2 = asyncArgument$11();
                                                                                return {
                                                                                  tag: "Throw",
                                                                                  _1: "parent failure"
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
function program(report) {
  const asyncArgument = Iris_Effect_Async.fork((environment) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Catch",
          _1: innerRelease(environment),
          _2: (caughtError) => ({
            tag: "Throw",
            _1: 1 | 0
          })
        };
      }
    };
  });
  return (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument(environment$1),
          _2: (mapped) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$1 = Iris_Effect_Async.fork((environment$2) => {
                  return {
                    tag: "Run",
                    _1: () => {
                      return {
                        tag: "Catch",
                        _1: innerCanceler(environment$2),
                        _2: (caughtError$1) => ({
                          tag: "Throw",
                          _1: 2 | 0
                        })
                      };
                    }
                  };
                });
                return {
                  tag: "Bind",
                  _1: asyncArgument$1(environment$1),
                  _2: (cancelled) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$2 = delay(20 | 0);
                        return {
                          tag: "Bind",
                          _1: asyncArgument$2(environment$1),
                          _2: ($unit) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$3 = Iris_Effect_Async.kill(7 | 0)(mapped);
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$3(environment$1),
                                  _2: ($unit$1) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const asyncArgument$4 = Iris_Effect_Async.kill(8 | 0)(cancelled);
                                        return {
                                          tag: "Bind",
                                          _1: asyncArgument$4(environment$1),
                                          _2: ($unit$2) => {
                                            return {
                                              tag: "Run",
                                              _1: () => {
                                                const asyncArgument$5 = Iris_Effect_Async.join(mapped);
                                                return {
                                                  tag: "Bind",
                                                  _1: {
                                                    tag: "Catch",
                                                    _1: {
                                                      tag: "Bind",
                                                      _1: asyncArgument$5(environment$1),
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
                                                  _2: (joinedMapped) => {
                                                    return {
                                                      tag: "Run",
                                                      _1: () => {
                                                        const asyncArgument$6 = Iris_Effect_Async.join(cancelled);
                                                        return {
                                                          tag: "Bind",
                                                          _1: {
                                                            tag: "Catch",
                                                            _1: {
                                                              tag: "Bind",
                                                              _1: asyncArgument$6(environment$1),
                                                              _2: (attemptValue$1) => ({
                                                                tag: "Pure",
                                                                _1: {
                                                                  tag: "Right",
                                                                  _1: attemptValue$1
                                                                }
                                                              })
                                                            },
                                                            _2: (caughtError$3) => ({
                                                              tag: "Pure",
                                                              _1: {
                                                                tag: "Left",
                                                                _1: caughtError$3
                                                              }
                                                            })
                                                          },
                                                          _2: (joinedCancelled) => {
                                                            return {
                                                              tag: "Run",
                                                              _1: () => {
                                                                const asyncArgument$7 = report(describeOuter(joinedMapped) + (", " + describeOuter(joinedCancelled)));
                                                                const $unit$3 = asyncArgument$7();
                                                                const asyncArgument$8 = children(report);
                                                                return {
                                                                  tag: "Bind",
                                                                  _1: {
                                                                    tag: "Catch",
                                                                    _1: asyncArgument$8(environment$1),
                                                                    _2: (caughtError$4) => {
                                                                      return {
                                                                        tag: "Run",
                                                                        _1: () => {
                                                                          const asyncArgument$9 = report("caught " + caughtError$4);
                                                                          const $unit$4 = asyncArgument$9();
                                                                          return {
                                                                            tag: "Throw",
                                                                            _1: 99 | 0
                                                                          };
                                                                        }
                                                                      };
                                                                    }
                                                                  },
                                                                  _2: ($int) => ({
                                                                    tag: "Pure",
                                                                    _1: Data_Unit.unit
                                                                  })
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
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
const innerRelease = /* @__PURE__ */ (() => {
  return Iris_Effect_Async.bracket((environment) => ({
    tag: "Pure",
    _1: Data_Unit.unit
  }))(($unit) => (environment$1) => ({
    tag: "Throw",
    _1: "release"
  }))(($unit$1) => {
    const asyncArgument = delay(1e3 | 0);
    return (environment$2) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: asyncArgument(environment$2),
            _2: (firstValue) => {
              return {
                tag: "Run",
                _1: () => {
                  return {
                    tag: "Pure",
                    _1: 9 | 0
                  };
                }
              };
            }
          };
        }
      };
    };
  });
})();
const innerCanceler = /* @__PURE__ */ (() => {
  return Iris_Effect_Async.async(($function) => {
    const $value = (environment) => ({
      tag: "Throw",
      _1: "cleanup"
    });
    return () => {
      return $value;
    };
  });
})();
