import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Fiber from "../Iris.Effect.Fiber/index.js";
import * as $foreign from "./foreign.js";
function say(report) {
  return (message) => {
    return Iris_Effect_Async.liftEffect(report(message));
  };
}
function killSelf(fiber) {
  const $function = Iris_Effect_Async.liftEffect;
  const $function$1 = Data_Functor.void(Effect.functorEffect);
  const $function$2 = Iris_Effect_Fiber.killFiber("self")(fiber);
  const $value = Data_Unit.unit;
  const $effect = () => {
    return $value;
  };
  return $function(/* @__PURE__ */ $function$1($function$2($effect)));
}
function withSelf(report) {
  return (body) => {
    const $function = Data_Functor.void(Effect.functorEffect);
    const $function$1 = Iris_Effect_Async.launch({ onUncaught: (failure) => report("uncaught " + failure) })({});
    const $function$2 = Iris_Effect_Async.liftEffect;
    const $effect = () => {
      return { value: "Nothing" };
    };
    const $closure = (self) => {
      const $closure$1 = (callback) => {
        const $action = later(callback({
          tag: "Right",
          _1: Data_Unit.unit
        }));
        return () => {
          const $unit = $action();
          const $value = /* @__PURE__ */ applicativeAsyncStringRowDictPure(Data_Unit.unit);
          return $value;
        };
      };
      const $closure$2 = ($unit$1) => {
        const $function$3 = Iris_Effect_Async.liftEffect;
        const $effect$1 = () => {
          return self.value;
        };
        const $closure$3 = (found) => {
          if (found.tag === "Just") {
            const { _1: me } = found;
            return body(me);
          }
          if (found === "Nothing") {
            return say(report)("missing fiber");
          }
          throw new Error("Pattern match failure");
        };
        return /* @__PURE__ */ bindAsyncStringRowDictBind($function$3($effect$1))($closure$3);
      };
      const $closure$4 = (fiber) => {
        const $function$4 = discardUnitDictDiscard(Iris_Effect_Async.bindAsync);
        const $function$5 = Iris_Effect_Async.liftEffect;
        const $value$1 = {
          tag: "Just",
          _1: fiber
        };
        const $effect$2 = () => {
          self.value = $value$1;
          return undefined;
        };
        const $closure$5 = ($unit$2) => {
          const $closure$6 = (exit) => {
            const $function$6 = say(report);
            let $result;
            $case: {
              if (exit.tag === "Succeeded") {
                $result = "succeeded";
                break $case;
              }
              if (exit.tag === "Failed") {
                const { _1: failure$1 } = exit;
                $result = "failed " + failure$1;
                break $case;
              }
              if (exit.tag === "Died") {
                $result = "died";
                break $case;
              }
              throw new Error("Pattern match failure");
            }
            return $function$6($result);
          };
          return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(fiber))($closure$6);
        };
        return /* @__PURE__ */ $function$4($function$5($effect$2))($closure$5);
      };
      return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(/* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.async($closure$1))($closure$2)))($closure$4);
    };
    return /* @__PURE__ */ $function($function$1(/* @__PURE__ */ bindAsyncStringRowDictBind($function$2($effect))($closure)));
  };
}
export function insideCatch(report) {
  return withSelf(report)((me) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.catch(/* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("before kill"))(($unit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(killSelf(me))(($unit$1) => say(report)("after kill"))))((failure) => say(report)("caught " + failure)))(($unit$2) => say(report)("after catch")));
}
export function beforeImmediateCallback(report) {
  const $closure = (me) => {
    const $closure$1 = ($unit) => {
      const $closure$2 = ($unit$1) => {
        const $closure$3 = (callback) => {
          const $action = callback({
            tag: "Right",
            _1: 5 | 0
          });
          return () => {
            const $unit$2 = $action();
            const $value = /* @__PURE__ */ applicativeAsyncStringRowDictPure(Data_Unit.unit);
            return $value;
          };
        };
        return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.async($closure$3))((value) => say(report)("after callback " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(value)));
      };
      return /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(killSelf(me))($closure$2);
    };
    return /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("before kill"))($closure$1);
  };
  return withSelf(report)($closure);
}
const later = $foreign["later"];
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
const applicativeAsyncStringRowDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync);
