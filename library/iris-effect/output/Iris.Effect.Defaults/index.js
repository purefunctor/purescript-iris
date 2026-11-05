import * as Control_Category from "../Control.Category/index.js";
import * as Control_Semigroupoid from "../Control.Semigroupoid/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Sync from "../Iris.Effect.Sync/index.js";
import * as Record_Builder from "../Record.Builder/index.js";
export function defaultService(dictionary) {
  return dictionary.defaultService;
}
export function defaultServices(dictionary) {
  return dictionary.defaultServices;
}
export function defaultServicesCons(isSymbolLabelDict) {
  return (defaultServiceServiceDict) => {
    return (defaultServicesTailServicesDict) => {
      return (lacksLabelServicesDict) => {
        return (consLabelServiceServicesExtendedDict) => {
          return { defaultServices: ($proxy) => /* @__PURE__ */ Control_Semigroupoid.compose(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert(consLabelServiceServicesExtendedDict)(lacksLabelServicesDict)(isSymbolLabelDict)("Proxy")(/* @__PURE__ */ defaultService(defaultServiceServiceDict)))(/* @__PURE__ */ defaultServices(defaultServicesTailServicesDict)("Proxy")) };
        };
      };
    };
  };
}
export function defaults(rowToListServicesListDict) {
  return (defaultServicesListServicesDict) => {
    return Record_Builder.buildFromScratch(/* @__PURE__ */ defaultServices(defaultServicesListServicesDict)("Proxy"));
  };
}
export function runDefaults(rowToListServicesListDict) {
  return (defaultServicesListServicesDict) => {
    return Iris_Effect_Sync.run(/* @__PURE__ */ defaults(rowToListServicesListDict)(defaultServicesListServicesDict));
  };
}
export function launchDefaults(rowToListServicesListDict) {
  return (defaultServicesListServicesDict) => {
    return (options) => Iris_Effect_Async.launch(options)(/* @__PURE__ */ defaults(rowToListServicesListDict)(defaultServicesListServicesDict));
  };
}
export const defaultServicesNil = { defaultServices: ($proxy) => /* @__PURE__ */ Control_Category.identity(Record_Builder.categoryBuilder) };
