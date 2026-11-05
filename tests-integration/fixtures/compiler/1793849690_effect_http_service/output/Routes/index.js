import * as AppError from "../AppError/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Handlers from "../Handlers/index.js";
import * as Iris_Effect_Json from "../Iris.Effect.Json/index.js";
import * as Logger from "../Logger/index.js";
import * as $foreign from "./foreign.js";
export function handle(services) {
  return (request) => {
    const asyncArgument = /* @__PURE__ */ route(services)(request);
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: {
              tag: "Catch",
              _1: {
                tag: "Bind",
                _1: asyncArgument(environment),
                _2: (attemptValue) => ({
                  tag: "Pure",
                  _1: {
                    tag: "Right",
                    _1: attemptValue
                  }
                })
              },
              _2: (caughtError) => ({
                tag: "Pure",
                _1: {
                  tag: "Left",
                  _1: caughtError
                }
              })
            },
            _2: (outcome) => {
              return {
                tag: "Run",
                _1: () => {
                  let $result;
                  $case: {
                    if (outcome.tag === "Right") {
                      const { _1: success } = outcome;
                      $result = success;
                      break $case;
                    }
                    if (outcome.tag === "Left") {
                      const { _1: error } = outcome;
                      $result = errorResponse(error);
                      break $case;
                    }
                    throw new Error("Pattern match failure");
                  }
                  const response = $result;
                  const asyncArgument$1 = /* @__PURE__ */ Logger.info(services)(request.method + (" " + (request.path + (" " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(response.status)))));
                  const $action = asyncArgument$1(environment);
                  const $unit = $action();
                  return {
                    tag: "Pure",
                    _1: response
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
function route(services) {
  return (request) => {
    const $scrutinee = request.method;
    const $scrutinee$1 = segments(request.path);
    if ($scrutinee === "GET" && Array.isArray($scrutinee$1) && $scrutinee$1.length === 1 && $scrutinee$1[0] === "health") {
      const asyncArgument = json(200 | 0)(Iris_Effect_Json.fromObject([{
        name: "status",
        value: Iris_Effect_Json.fromString("ok")
      }]));
      return (environment) => ({
        tag: "Pure",
        _1: asyncArgument
      });
    }
    if ($scrutinee === "GET" && Array.isArray($scrutinee$1) && $scrutinee$1.length === 1 && $scrutinee$1[0] === "crash") {
      const asyncArgument$1 = defect("route crashed");
      return (environment$1) => {
        return {
          tag: "Run",
          _1: () => {
            const $unit = asyncArgument$1();
            const asyncArgument$2 = json(200 | 0)(Iris_Effect_Json.fromObject([]));
            const $result = asyncArgument$2;
            return {
              tag: "Pure",
              _1: $result
            };
          }
        };
      };
    }
    if ($scrutinee === "GET" && Array.isArray($scrutinee$1) && $scrutinee$1.length === 2 && $scrutinee$1[0] === "products") {
      const identifier = $scrutinee$1[1];
      const composeOuter = json(200 | 0);
      const asyncArgument$3 = /* @__PURE__ */ Handlers.getProduct(services)(identifier);
      return (environment$2) => {
        return {
          tag: "Run",
          _1: () => {
            return {
              tag: "Bind",
              _1: asyncArgument$3(environment$2),
              _2: (mappedValue) => ({
                tag: "Pure",
                _1: /* @__PURE__ */ composeOuter(/* @__PURE__ */ Handlers.encodeProduct(mappedValue))
              })
            };
          }
        };
      };
    }
    if ($scrutinee === "POST" && Array.isArray($scrutinee$1) && $scrutinee$1.length === 1 && $scrutinee$1[0] === "orders") {
      const composeOuter$1 = json(201 | 0);
      const asyncArgument$4 = /* @__PURE__ */ Handlers.createOrder(services)(request.body);
      return (environment$3) => {
        return {
          tag: "Run",
          _1: () => {
            return {
              tag: "Bind",
              _1: asyncArgument$4(environment$3),
              _2: (mappedValue$1) => ({
                tag: "Pure",
                _1: /* @__PURE__ */ composeOuter$1(/* @__PURE__ */ Handlers.encodeOrder(mappedValue$1))
              })
            };
          }
        };
      };
    }
    if ($scrutinee === "GET" && Array.isArray($scrutinee$1) && $scrutinee$1.length === 2 && $scrutinee$1[0] === "orders") {
      const number = $scrutinee$1[1];
      const composeOuter$2 = json(200 | 0);
      const asyncArgument$5 = /* @__PURE__ */ Handlers.getOrder(services)(number);
      return (environment$4) => {
        return {
          tag: "Run",
          _1: () => {
            return {
              tag: "Bind",
              _1: asyncArgument$5(environment$4),
              _2: (mappedValue$2) => ({
                tag: "Pure",
                _1: /* @__PURE__ */ composeOuter$2(/* @__PURE__ */ Handlers.encodeOrder(mappedValue$2))
              })
            };
          }
        };
      };
    }
    const method = $scrutinee;
    const asyncArgument$6 = {
      tag: "NotFound",
      _1: "route " + (method + (" " + request.path))
    };
    return (environment$5) => ({
      tag: "Throw",
      _1: asyncArgument$6
    });
  };
}
export function errorResponse(error) {
  return json(AppError.status(error))(AppError.toJson(error));
}
function json(status) {
  return (body) => {
    return {
      status,
      headers: [{
        name: "content-type",
        value: "application/json"
      }],
      body: Iris_Effect_Json.stringify(body)
    };
  };
}
const segments = $foreign["segments"];
const defect = $foreign["defect"];
