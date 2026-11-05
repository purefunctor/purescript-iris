import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as $foreign from "./foreign.js";
function throughMonad(monadMonadDict) {
  return (action) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(action)((value) => /* @__PURE__ */ Control_Applicative.pure(/* @__PURE__ */ monadMonadDict.Applicative0())(value * (3 | 0) | 0));
}
function program(services) {
  return (input) => {
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const offset = services["Main.Counter"](environment);
          const asyncArgument = step("first")(input);
          const first = asyncArgument();
          const $function = throughMonad(Iris_Effect_Async.monadAsync);
          const asyncArgument$2 = first + offset | 0;
          const $result = (environment$1) => ({
            tag: "Pure",
            _1: asyncArgument$2
          });
          const asyncArgument$1 = /* @__PURE__ */ $function($result);
          const $value$2 = {
            tag: "Bind",
            _1: asyncArgument$1(environment),
            _2: (adjusted) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$3 = step("left")(adjusted);
                  const asyncArgument$4 = step("right")(offset);
                  const mappedValue = asyncArgument$3();
                  const appliedFunction = ((left) => (right) => left - right | 0)(mappedValue);
                  const appliedArgument = asyncArgument$4();
                  const result = appliedFunction(appliedArgument);
                  if (result > (0 | 0)) {
                    const $value = {
                      tag: "Pure",
                      _1: result
                    };
                    return $value;
                  } else {
                    const $value$1 = {
                      tag: "Throw",
                      _1: "negative result"
                    };
                    return $value$1;
                  }
                }
              };
            }
          };
          return $value$2;
        }
      };
    };
  };
}
export function runOrdered($unit) {
  const environment = mark("environment")({ choice: 7 | 0 });
  const stored = mark("program")({ action: /* @__PURE__ */ program({ "Main.Counter": (environment$1) => environment$1.choice })(11 | 0) });
  const $function = Iris_Effect_Async.launch({ onUncaught: failure })(environment);
  const asyncArgument = stored.action;
  const $result = (environment$2) => {
    return {
      tag: "Run",
      _1: () => {
        const $value$1 = {
          tag: "Bind",
          _1: asyncArgument(environment$2),
          _2: (result) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$1 = complete(result);
                const value = asyncArgument$1();
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
  };
  const $action = $function($result);
  return () => {
    const $fiber = $action();
    const $value$2 = Data_Unit.unit;
    return $value$2;
  };
}
const step = $foreign["step"];
const complete = $foreign["complete"];
const failure = $foreign["failure"];
const mark = $foreign["mark"];
export const provided = /* @__PURE__ */ (() => {
  const $function = Iris_Effect_Async.launch({ onUncaught: failure })({});
  const asyncArgument = { renamed: 3 | 0 };
  const asyncArgument$1 = /* @__PURE__ */ program({ "Main.Counter": (environment) => environment.renamed })(5 | 0);
  const $result = (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        const $value$1 = {
          tag: "Bind",
          _1: asyncArgument$1(asyncArgument),
          _2: (result) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$2 = complete(result);
                const value = asyncArgument$2();
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
  };
  const $action = $function($result);
  return () => {
    const $fiber = $action();
    const $value$2 = Data_Unit.unit;
    return $value$2;
  };
})();
