import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as $foreign from "./foreign.js";
function lowered(services) {
  return (environment) => {
    return {
      tag: "Run",
      _1: () => {
        const offset = services["Main.Offset"](environment);
        const asyncArgument = record(offset + (1 | 0) | 0);
        const first = asyncArgument();
        const asyncArgument$1 = record(first + (1 | 0) | 0);
        const second = asyncArgument$1();
        return {
          tag: "Bind",
          _1: pause(environment),
          _2: (third) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$2 = record(second + third | 0);
                const fourth = asyncArgument$2();
                const asyncArgument$3 = complete(fourth);
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
  };
}
function generic(monadMonadDict) {
  return (step) => (suspend) => (finish) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(step(4 | 0))((first) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(step(first + (1 | 0) | 0))((second) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(suspend)((third) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(step(second + third | 0))((fourth) => finish(fourth)))));
}
const record = $foreign["record"];
const later = $foreign["later"];
const complete = $foreign["complete"];
const failed = $foreign["failed"];
const pause = /* @__PURE__ */ (() => {
  return Iris_Effect_Async.async((callback) => {
    const $action = later((composeArgument) => /* @__PURE__ */ callback({
      tag: "Right",
      _1: composeArgument
    }));
    return () => {
      const $unit = $action();
      return (environment) => ({
        tag: "Pure",
        _1: Data_Unit.unit
      });
    };
  });
})();
export const runLowered = /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: failed })({ offset: 3 | 0 })(/* @__PURE__ */ lowered({ "Main.Offset": (environment) => environment.offset })));
export const runGeneric = /* @__PURE__ */ (() => {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: failed })({})(/* @__PURE__ */ generic(Iris_Effect_Async.monadAsync)((composeArgument) => {
    const asyncArgument = /* @__PURE__ */ record(composeArgument);
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
  })(pause)((composeArgument$1) => {
    const asyncArgument$1 = /* @__PURE__ */ complete(composeArgument$1);
    return (environment$1) => {
      return {
        tag: "Run",
        _1: () => {
          const value$1 = asyncArgument$1();
          return {
            tag: "Pure",
            _1: value$1
          };
        }
      };
    };
  })));
})();
