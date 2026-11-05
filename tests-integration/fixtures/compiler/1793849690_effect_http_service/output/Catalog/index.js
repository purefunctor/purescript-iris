import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Iris_Effect_Http from "../Iris.Effect.Http/index.js";
import * as Iris_Effect_Json from "../Iris.Effect.Json/index.js";
export const Missing = "Missing";
export const Unavailable = ($value0) => ({
  tag: "Unavailable",
  _1: $value0
});
export const Abandoned = "Abandoned";
export function make($record) {
  const baseUrl = $record.baseUrl;
  return (http) => {
    return {
      baseUrl,
      http
    };
  };
}
export function listing(services) {
  return (identifier) => {
    const asyncArgument = /* @__PURE__ */ fetchJson(services)("/catalog/" + identifier);
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: asyncArgument(environment),
            _2: (json) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$1 = decode(/* @__PURE__ */ bindEitherStringDictBind(Data_Either.note("missing name")(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("name")(json))(Iris_Effect_Json.toString)))((name) => /* @__PURE__ */ bindEitherStringDictBind(Data_Either.note("missing price")(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("price")(json))(Iris_Effect_Json.toInt)))((price) => /* @__PURE__ */ Control_Applicative.pure(Data_Either.applicativeEither)({
                    name,
                    price
                  }))));
                  return {
                    tag: "Bind",
                    _1: asyncArgument$1(environment),
                    _2: (decoded) => ({
                      tag: "Pure",
                      _1: decoded
                    })
                  };
                }
              };
            }
          };
        }
      };
    };
  };
}
export function stock(services) {
  return (identifier) => {
    const asyncArgument = /* @__PURE__ */ fetchJson(services)("/stock/" + identifier);
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: asyncArgument(environment),
            _2: (json) => decode(Data_Either.note("missing available")(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("available")(json))(Iris_Effect_Json.toInt)))(environment)
          };
        }
      };
    };
  };
}
function decode(section264) {
  if (section264.tag === "Right") {
    const { _1: value } = section264;
    return (environment) => ({
      tag: "Pure",
      _1: value
    });
  }
  if (section264.tag === "Left") {
    const { _1: message } = section264;
    const asyncArgument = {
      tag: "Unavailable",
      _1: "invalid upstream payload: " + message
    };
    return (environment$1) => ({
      tag: "Throw",
      _1: asyncArgument
    });
  }
  throw new Error("Pattern match failure");
}
function fetchJson(services) {
  return (path) => {
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const catalog = services["Catalog.Catalog"](environment);
          const asyncArgument = { http: catalog.http };
          const composeInner = /* @__PURE__ */ Data_Show.show(Iris_Effect_Http.showHttpError);
          const asyncArgument$1 = /* @__PURE__ */ Iris_Effect_Http.get({ "Iris.Effect.Http.Http": (environment$1) => environment$1.http })(catalog.baseUrl + path);
          return {
            tag: "Bind",
            _1: {
              tag: "Catch",
              _1: asyncArgument$1(asyncArgument),
              _2: (caughtError) => ({
                tag: "Throw",
                _1: {
                  tag: "Unavailable",
                  _1: /* @__PURE__ */ composeInner(caughtError)
                }
              })
            },
            _2: (response) => {
              return {
                tag: "Run",
                _1: () => {
                  const $scrutinee = response.status;
                  if ($scrutinee === (200 | 0)) {
                    return decode(Iris_Effect_Json.parse(response.body))(environment);
                  }
                  if ($scrutinee === (404 | 0)) {
                    return {
                      tag: "Throw",
                      _1: "Missing"
                    };
                  }
                  const status = $scrutinee;
                  const asyncArgument$2 = {
                    tag: "Unavailable",
                    _1: "upstream status " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(status)
                  };
                  return {
                    tag: "Throw",
                    _1: asyncArgument$2
                  };
                }
              };
            }
          };
        }
      };
    };
  };
}
const bindEitherStringDictBind = /* @__PURE__ */ Control_Bind.bind(Data_Either.bindEither);
const bindMaybeDictBind = /* @__PURE__ */ Control_Bind.bind(Data_Maybe.bindMaybe);
