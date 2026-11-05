import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
function nestedForks(depth) {
  return (environment) => {
    return {
      tag: "Run",
      _1: () => {
        if (depth === (0 | 0)) {
          return {
            tag: "Pure",
            _1: 719 | 0
          };
        } else {
          const asyncArgument = Iris_Effect_Async.fork(nestedForks(depth - (1 | 0) | 0));
          return {
            tag: "Bind",
            _1: asyncArgument(environment),
            _2: (child) => Iris_Effect_Async.join(child)(environment)
          };
        }
      }
    };
  };
}
function countdown(remaining) {
  if (remaining === (0 | 0)) {
    return (environment) => ({
      tag: "Pure",
      _1: 0 | 0
    });
  } else {
    return (environment$1) => {
      return {
        tag: "Run",
        _1: () => {
          const asyncArgument = countdown(remaining - (1 | 0) | 0);
          return {
            tag: "Bind",
            _1: asyncArgument(environment$1),
            _2: (next) => ({
              tag: "Pure",
              _1: next + (1 | 0) | 0
            })
          };
        }
      };
    };
  }
}
function program(report) {
  const asyncArgument = nestedForks(1e4 | 0);
  return (environment) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument(environment),
          _2: (forked) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$1 = report("nested forks " + /* @__PURE__ */ showIntDictShow(forked));
                const $unit = asyncArgument$1();
                const asyncArgument$2 = countdown(1e5 | 0);
                return {
                  tag: "Bind",
                  _1: asyncArgument$2(environment),
                  _2: (counted) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$3 = report("binds " + /* @__PURE__ */ showIntDictShow(counted));
                        const value = asyncArgument$3();
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
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
