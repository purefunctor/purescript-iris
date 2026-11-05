import * as Data_Array from "../Data.Array/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semiring from "../Data.Semiring/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
function sequenced(effects) {
  return Data_Array.foldl((action) => {
    return ($int) => {
      const $effect = () => {
        const section123 = effects.value;
        const $value = section123 + (1 | 0) | 0;
        return effects.value = $value;
      };
      const asyncArgument = $effect;
      return (environment) => {
        return {
          tag: "Run",
          _1: () => {
            const $value$2 = {
              tag: "Bind",
              _1: action(environment),
              _2: (firstValue) => {
                return {
                  tag: "Run",
                  _1: () => {
                    const secondValue = asyncArgument();
                    const $value$1 = {
                      tag: "Pure",
                      _1: secondValue
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
  })((environment$1) => ({
    tag: "Pure",
    _1: 0 | 0
  }))(Data_Array.range(1 | 0)(1e5 | 0));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (failure) => report("uncaught " + failure) })({})((environment) => {
    return {
      tag: "Run",
      _1: () => {
        const $value$4 = {
          tag: "Bind",
          _1: counted(environment),
          _2: (total) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument = report("total " + /* @__PURE__ */ showIntDictShow(total));
                const $unit = asyncArgument();
                const $effect = () => {
                  return { value: 0 | 0 };
                };
                const asyncArgument$1 = $effect;
                const effects = asyncArgument$1();
                const asyncArgument$2 = sequenced(effects);
                const $value$3 = {
                  tag: "Bind",
                  _1: asyncArgument$2(environment),
                  _2: (last) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const $effect$1 = () => {
                          return effects.value;
                        };
                        const asyncArgument$3 = $effect$1;
                        const performed = asyncArgument$3();
                        const asyncArgument$4 = report("effects " + (/* @__PURE__ */ showIntDictShow(last) + (" " + /* @__PURE__ */ showIntDictShow(performed))));
                        const $unit$1 = asyncArgument$4();
                        const $value$2 = {
                          tag: "Bind",
                          _1: mapped(environment),
                          _2: (mappedTotal) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$5 = report("mapped " + /* @__PURE__ */ showIntDictShow(mappedTotal));
                                const $unit$2 = asyncArgument$5();
                                const $value$1 = {
                                  tag: "Bind",
                                  _1: applied(environment),
                                  _2: (appliedTotal) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const asyncArgument$6 = report("applied " + /* @__PURE__ */ showIntDictShow(appliedTotal));
                                        const value = asyncArgument$6();
                                        const $value = {
                                          tag: "Pure",
                                          _1: value
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
                  }
                };
                return $value$3;
              }
            };
          }
        };
        return $value$4;
      }
    };
  }));
}
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
const counted = /* @__PURE__ */ (() => {
  return Data_Array.foldl((action) => {
    return ($int) => {
      return (environment) => {
        return {
          tag: "Run",
          _1: () => {
            const $value = {
              tag: "Bind",
              _1: action(environment),
              _2: (value) => ({
                tag: "Pure",
                _1: value + (1 | 0) | 0
              })
            };
            return $value;
          }
        };
      };
    };
  })((environment$1) => ({
    tag: "Pure",
    _1: 0 | 0
  }))(Data_Array.range(1 | 0)(1e5 | 0));
})();
const mapped = /* @__PURE__ */ (() => {
  return Data_Array.foldl((action) => {
    return ($int) => {
      return (environment) => {
        return {
          tag: "Run",
          _1: () => {
            const $value = {
              tag: "Bind",
              _1: action(environment),
              _2: (mappedValue) => ({
                tag: "Pure",
                _1: mappedValue + (1 | 0) | 0
              })
            };
            return $value;
          }
        };
      };
    };
  })((environment$1) => ({
    tag: "Pure",
    _1: 0 | 0
  }))(Data_Array.range(1 | 0)(1e5 | 0));
})();
const applied = /* @__PURE__ */ (() => {
  return Data_Array.foldl((action) => {
    return ($int) => {
      const asyncArgument = /* @__PURE__ */ Data_Semiring.add(Data_Semiring.semiringInt);
      return (environment) => {
        return {
          tag: "Run",
          _1: () => {
            const $value$1 = {
              tag: "Bind",
              _1: action(environment),
              _2: (mappedValue) => {
                return {
                  tag: "Run",
                  _1: () => {
                    const appliedFunction = asyncArgument(mappedValue);
                    const $value = {
                      tag: "Pure",
                      _1: appliedFunction(1 | 0)
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
    };
  })((environment$1) => ({
    tag: "Pure",
    _1: 0 | 0
  }))(Data_Array.range(1 | 0)(1e5 | 0));
})();
