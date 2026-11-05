import * as Data_Show from "../Data.Show/index.js";
function greeting(hasNameServicesDict) {
  return (environment) => {
    const $value = hasNameServicesDict.getService(environment);
    return () => {
      const $value$1 = "hello " + $value;
      return $value$1;
    };
  };
}
function say(report) {
  return (message) => {
    const syncArgument = report(message);
    return (environment) => syncArgument;
  };
}
function program(hasNameServicesDict) {
  return (hasCountServicesDict) => {
    return (report) => {
      const syncArgument = say(report)("start");
      return (environment) => {
        const $action = syncArgument(environment);
        return () => {
          const $unit = $action();
          const syncArgument$1 = /* @__PURE__ */ greeting(hasNameServicesDict);
          const $action$1 = syncArgument$1(environment);
          const outer = $action$1();
          const syncArgument$2 = /* @__PURE__ */ greeting({ getService: ($services) => $services.renamed });
          const $action$2 = syncArgument$2({ renamed: "inner" });
          const inner = $action$2();
          const $value = hasCountServicesDict.getService(environment);
          const syncArgument$3 = /* @__PURE__ */ greeting(hasNameServicesDict);
          const syncArgument$4 = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
          const $function = (left) => (right) => left + (" & " + right);
          const $action$3 = syncArgument$3(environment);
          let combined;
          let $function$1;
          $function$1 = $function($action$3());
          let $argument;
          let $value$1;
          $value$1 = $value;
          $argument = syncArgument$4($value$1);
          combined = $function$1($argument);
          return say(report)(outer + (", " + (inner + (", " + combined))))(environment)();
        };
      };
    };
  };
}
export function run(report) {
  const action = /* @__PURE__ */ program({ getService: ($services) => $services.name })({ getService: ($services) => $services.count })(report)({
    name: "outer",
    count: 2 | 0
  });
  const $action = report("built");
  return () => {
    const $unit = $action();
    const $unit$1 = action();
    return action();
  };
}
