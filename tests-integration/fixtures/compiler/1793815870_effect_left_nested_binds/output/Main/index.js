import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Array from "../Data.Array/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semiring from "../Data.Semiring/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
function sequenced(effects) {
  const $closure = (action) => {
    return ($int) => {
      const $function = Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(action);
      const $function$1 = Iris_Effect_Async.liftEffect;
      const $effect = () => {
        const section123 = effects.value;
        const $value = section123 + (1 | 0) | 0;
        return effects.value = $value;
      };
      return /* @__PURE__ */ $function($function$1($effect));
    };
  };
  return Data_Array.foldl($closure)(/* @__PURE__ */ applicativeAsyncStringRowDictPure(0 | 0))(Data_Array.range(1 | 0)(1e5 | 0));
}
export function run(report) {
  const $closure = (total) => {
    const $closure$1 = ($unit) => {
      const $function = Iris_Effect_Async.liftEffect;
      const $effect = () => {
        return { value: 0 | 0 };
      };
      const $closure$2 = (effects) => {
        const $closure$3 = (last) => {
          const $function$1 = Iris_Effect_Async.liftEffect;
          const $effect$1 = () => {
            return effects.value;
          };
          return /* @__PURE__ */ bindAsyncStringRowDictBind($function$1($effect$1))((performed) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.liftEffect(report("effects " + (/* @__PURE__ */ showIntDictShow(last) + (" " + /* @__PURE__ */ showIntDictShow(performed))))))(($unit$1) => /* @__PURE__ */ bindAsyncStringRowDictBind(mapped)((mappedTotal) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.liftEffect(report("mapped " + /* @__PURE__ */ showIntDictShow(mappedTotal))))(($unit$2) => /* @__PURE__ */ bindAsyncStringRowDictBind(applied)((appliedTotal) => Iris_Effect_Async.liftEffect(report("applied " + /* @__PURE__ */ showIntDictShow(appliedTotal))))))));
        };
        return /* @__PURE__ */ bindAsyncStringRowDictBind(sequenced(effects))($closure$3);
      };
      return /* @__PURE__ */ bindAsyncStringRowDictBind($function($effect))($closure$2);
    };
    return /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.liftEffect(report("total " + /* @__PURE__ */ showIntDictShow(total))))($closure$1);
  };
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (failure) => report("uncaught " + failure) })({})(/* @__PURE__ */ bindAsyncStringRowDictBind(counted)($closure)));
}
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const applicativeAsyncStringRowDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync);
const functorAsyncStringRowDictMap = /* @__PURE__ */ Data_Functor.map(Iris_Effect_Async.functorAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
const counted = Data_Array.foldl((action) => ($int) => /* @__PURE__ */ bindAsyncStringRowDictBind(action)((value) => /* @__PURE__ */ applicativeAsyncStringRowDictPure(value + (1 | 0) | 0)))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(0 | 0))(Data_Array.range(1 | 0)(1e5 | 0));
const mapped = Data_Array.foldl((action) => ($int) => /* @__PURE__ */ functorAsyncStringRowDictMap((section167) => section167 + (1 | 0) | 0)(action))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(0 | 0))(Data_Array.range(1 | 0)(1e5 | 0));
const applied = Data_Array.foldl((action) => ($int) => /* @__PURE__ */ Control_Apply.apply(Iris_Effect_Async.applyAsync)(/* @__PURE__ */ functorAsyncStringRowDictMap(/* @__PURE__ */ Data_Semiring.add(Data_Semiring.semiringInt))(action))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(1 | 0)))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(0 | 0))(Data_Array.range(1 | 0)(1e5 | 0));
