import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Iris_Effect_Sync from "../Iris.Effect.Sync/index.js";
function greeting(hasNameServicesDict) {
  return /* @__PURE__ */ bindSyncServicesDictBind(/* @__PURE__ */ Iris_Effect_Sync.service(hasNameServicesDict))((name) => /* @__PURE__ */ applicativeSyncServicesDictPure("hello " + name));
}
function say(report) {
  return (message) => {
    return Iris_Effect_Sync.liftEffect(report(message));
  };
}
function program(hasNameServicesDict) {
  return (hasCountServicesDict) => {
    return (report) => /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit)(Iris_Effect_Sync.bindSync)(say(report)("start"))(($unit) => /* @__PURE__ */ bindSyncServicesDictBind(/* @__PURE__ */ greeting(hasNameServicesDict))((outer) => /* @__PURE__ */ bindSyncServicesDictBind(Iris_Effect_Sync.provide({ renamed: "inner" })(/* @__PURE__ */ greeting({ getService: ($services) => $services.renamed })))((inner) => /* @__PURE__ */ bindSyncServicesDictBind(/* @__PURE__ */ Iris_Effect_Sync.service(hasCountServicesDict))((count) => /* @__PURE__ */ bindSyncServicesDictBind(/* @__PURE__ */ Control_Apply.apply(Iris_Effect_Sync.applySync)(/* @__PURE__ */ functorSyncServicesDictMap((left) => (right) => left + (" & " + right))(/* @__PURE__ */ greeting(hasNameServicesDict)))(/* @__PURE__ */ functorSyncServicesDictMap(/* @__PURE__ */ Data_Show.show(Data_Show.showInt))(/* @__PURE__ */ applicativeSyncServicesDictPure(count))))((combined) => say(report)(outer + (", " + (inner + (", " + combined)))))))));
  };
}
export function run(report) {
  const action = Iris_Effect_Sync.run({
    name: "outer",
    count: 2 | 0
  })(/* @__PURE__ */ program({ getService: ($services) => $services.name })({ getService: ($services) => $services.count })(report));
  const $action = report("built");
  return () => {
    const $unit = $action();
    const $unit$1 = action();
    return action();
  };
}
const bindSyncServicesDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Sync.bindSync);
const applicativeSyncServicesDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Sync.applicativeSync);
const functorSyncServicesDictMap = /* @__PURE__ */ Data_Functor.map(Iris_Effect_Sync.functorSync);
