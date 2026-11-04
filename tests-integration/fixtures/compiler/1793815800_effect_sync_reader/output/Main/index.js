import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Iris_Effect_Sync from "../Iris.Effect.Sync/index.js";
function say(report) {
  return (message) => {
    return Iris_Effect_Sync.liftEffect(report(message));
  };
}
function program(report) {
  const $closure = ($unit) => {
    const $closure$1 = (outer) => {
      const $closure$2 = (inner) => {
        const $closure$3 = ($record) => {
          const count = $record.count;
          return () => {
            return count;
          };
        };
        return /* @__PURE__ */ bindSyncRowDictBind(Iris_Effect_Sync.make($closure$3))((count$1) => /* @__PURE__ */ bindSyncRowDictBind(/* @__PURE__ */ Control_Apply.apply(Iris_Effect_Sync.applySync)(/* @__PURE__ */ functorSyncRowDictMap((left) => (right) => left + (" & " + right))(greeting))(/* @__PURE__ */ functorSyncRowDictMap(/* @__PURE__ */ Data_Show.show(Data_Show.showInt))(/* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Sync.applicativeSync)(count$1))))((combined) => say(report)(outer + (", " + (inner + (", " + combined))))));
      };
      return /* @__PURE__ */ bindSyncRowDictBind(Iris_Effect_Sync.provide({ name: "inner" })(greeting))($closure$2);
    };
    return /* @__PURE__ */ bindSyncRowDictBind(greeting)($closure$1);
  };
  return /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit)(Iris_Effect_Sync.bindSync)(say(report)("start"))($closure);
}
export function run(report) {
  const action = Iris_Effect_Sync.run({
    name: "outer",
    count: 2 | 0
  })(program(report));
  const $action = report("built");
  return () => {
    const $unit = $action();
    const $unit$1 = action();
    return action();
  };
}
const bindSyncRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Sync.bindSync);
const functorSyncRowDictMap = /* @__PURE__ */ Data_Functor.map(Iris_Effect_Sync.functorSync);
const greeting = /* @__PURE__ */ (() => {
  const $closure = ($record) => {
    const name = $record.name;
    const $value = "hello " + name;
    return () => {
      return $value;
    };
  };
  return Iris_Effect_Sync.make($closure);
})();
