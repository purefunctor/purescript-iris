import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as $foreign from "./foreign.js";
export const NetworkError = ($value0) => ({
  tag: "NetworkError",
  _1: $value0
});
export function request(services) {
  return (parameters) => {
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const http = services["Iris.Effect.Http.Http"](environment);
          return Iris_Effect_Async.async((callback) => {
            const $action = http.request(parameters)(callback);
            return () => {
              const cancel = $action();
              return (environment$1) => {
                return {
                  tag: "Run",
                  _1: () => {
                    const value = cancel();
                    return {
                      tag: "Pure",
                      _1: value
                    };
                  }
                };
              };
            };
          })(environment);
        }
      };
    };
  };
}
export function get(services) {
  return (url) => /* @__PURE__ */ request(services)({
    method: "GET",
    url,
    headers: [],
    body: "Nothing"
  });
}
export function fromForeignClient(client) {
  return { request: (parameters) => (callback) => client({
    method: parameters.method,
    url: parameters.url,
    headers: parameters.headers,
    body: Data_Maybe.fromMaybe("")(parameters.body),
    hasBody: Data_Maybe.isJust(parameters.body)
  })((composeArgument) => /* @__PURE__ */ callback(/* @__PURE__ */ ((composeArgument$1) => ({
    tag: "Left",
    _1: {
      tag: "NetworkError",
      _1: composeArgument$1
    }
  }))(composeArgument)))((composeArgument$2) => /* @__PURE__ */ callback({
    tag: "Right",
    _1: composeArgument$2
  })) };
}
const fetchClient = $foreign["fetchClient"];
export const showHttpError = /* @__PURE__ */ (() => {
  return { show: ($httpError) => {
    if ($httpError.tag === "NetworkError") {
      const { _1: message } = $httpError;
      return "(NetworkError " + (/* @__PURE__ */ Data_Show.show(Data_Show.showString)(message) + ")");
    } else {
      throw new Error("Pattern match failure");
    }
  } };
})();
const $default = fromForeignClient(fetchClient);
export const defaultServiceHttp = { defaultService: $default };
export { $default as "default" };
