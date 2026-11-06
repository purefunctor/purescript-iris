import * as Data_Show from "../Data.Show/index.js";
import * as Logging from "../Logging/index.js";
function handle(services) {
  return (request) => {
    return (environment) => {
      const $value = services["Main.Clock"](environment);
      return () => {
        return (/* @__PURE__ */ Logging.logLine(services)(/* @__PURE__ */ Data_Show.show(Data_Show.showInt)($value) + (" " + request))(environment))();
      };
    };
  };
}
function labelled(showValueDict) {
  return (hasLabelValueServicesDict) => {
    return (services) => {
      return (environment) => {
        const $value = hasLabelValueServicesDict.getService(environment);
        return () => {
          return (/* @__PURE__ */ Logging.logLine(services)(/* @__PURE__ */ Data_Show.show(showValueDict)($value))(environment))();
        };
      };
    };
  };
}
export function run(write) {
  const syncArgument = /* @__PURE__ */ handle({
    "Logging.Logger": (environment) => environment.log,
    "Main.Clock": (environment$1) => environment$1.clock
  })("first");
  const environment$2 = {
    clock: 12 | 0,
    log: write,
    label: true
  };
  const $action = syncArgument(environment$2);
  return () => {
    const $unit = $action();
    return (/* @__PURE__ */ labelled(Data_Show.showBoolean)({ getService: (environment$3) => environment$3.label })({ "Logging.Logger": (environment$4) => environment$4.log })(environment$2))();
  };
}
