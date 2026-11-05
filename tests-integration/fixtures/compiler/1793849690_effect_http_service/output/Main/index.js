import * as Catalog from "../Catalog/index.js";
import * as Client from "../Client/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Http from "../Iris.Effect.Http/index.js";
import * as Iris_Effect_Http_Node from "../Iris.Effect.Http.Node/index.js";
import * as OrderStore from "../OrderStore/index.js";
import * as ProductCache from "../ProductCache/index.js";
import * as Routes from "../Routes/index.js";
import * as Test_HttpServer from "../Test.HttpServer/index.js";
export function start(options) {
  const $action = ProductCache.make({ capacity: options.cacheCapacity });
  return () => {
    const cache = $action();
    const $action$1 = OrderStore.make;
    const orders = $action$1();
    const environment = {
      server: Test_HttpServer.default,
      catalog: Catalog.make({ baseUrl: options.upstream })(Iris_Effect_Http_Node.client),
      cache,
      orders,
      logger: { write: options.log }
    };
    const $function = Data_Functor.void(Effect.functorEffect);
    const $function$1 = Iris_Effect_Async.launch;
    const composeOuter = options.onFailure;
    const composeInner = /* @__PURE__ */ Data_Show.show(Test_HttpServer.showServerError);
    const $result = (composeArgument) => /* @__PURE__ */ composeOuter(/* @__PURE__ */ composeInner(composeArgument));
    const $function$2 = $function$1({ onUncaught: $result })(environment);
    const asyncArgument = /* @__PURE__ */ Test_HttpServer.serve({ "Test.HttpServer.HttpServer": (environment$1) => environment$1.server })({
      host: options.host,
      port: options.port,
      onError: Routes.errorResponse
    })(/* @__PURE__ */ Routes.handle({
      "Catalog.Catalog": (environment$2) => environment$2.catalog,
      "ProductCache.ProductCache": (environment$3) => environment$3.cache,
      "OrderStore.OrderStore": (environment$4) => environment$4.orders,
      "Logger.Logger": (environment$5) => environment$5.logger
    }));
    const $result$1 = (environment$6) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: asyncArgument(environment$6),
            _2: (server) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$1 = options.onListening({
                    port: Test_HttpServer.address(server).port,
                    stop: ((server$1) => {
                      return (done) => {
                        const $function$3 = Data_Functor.void(Effect.functorEffect);
                        const $function$4 = Iris_Effect_Async.launch({ onUncaught: options.onFailure })(environment);
                        const asyncArgument$2 = Test_HttpServer.close(server$1);
                        const $result$2 = (environment$7) => {
                          return {
                            tag: "Run",
                            _1: () => {
                              return {
                                tag: "Bind",
                                _1: asyncArgument$2(environment$7),
                                _2: ($unit) => {
                                  return {
                                    tag: "Run",
                                    _1: () => {
                                      const value = done();
                                      return {
                                        tag: "Pure",
                                        _1: value
                                      };
                                    }
                                  };
                                }
                              };
                            }
                          };
                        };
                        return /* @__PURE__ */ $function$3($function$4($result$2));
                      };
                    })(server)
                  });
                  const value$1 = asyncArgument$1();
                  return {
                    tag: "Pure",
                    _1: value$1
                  };
                }
              };
            }
          };
        }
      };
    };
    return (/* @__PURE__ */ $function($function$2($result$1)))();
  };
}
export function runScenario($record) {
  const baseUrl = $record.baseUrl;
  const onFailure = $record.onFailure;
  const onResult = $record.onResult;
  const $function = Data_Functor.void(Effect.functorEffect);
  const $function$1 = Iris_Effect_Async.launch;
  const composeInner = /* @__PURE__ */ Data_Show.show(Iris_Effect_Http.showHttpError);
  const $result = (composeArgument) => /* @__PURE__ */ onFailure(/* @__PURE__ */ composeInner(composeArgument));
  const $function$2 = $function$1({ onUncaught: $result })({ http: Iris_Effect_Http_Node.client });
  const asyncArgument = /* @__PURE__ */ Client.scenario({ "Iris.Effect.Http.Http": (environment) => environment.http })(baseUrl);
  const $result$1 = (environment$1) => {
    return {
      tag: "Run",
      _1: () => {
        return {
          tag: "Bind",
          _1: asyncArgument(environment$1),
          _2: (lines) => {
            return {
              tag: "Run",
              _1: () => {
                const asyncArgument$1 = onResult(lines);
                const value = asyncArgument$1();
                return {
                  tag: "Pure",
                  _1: value
                };
              }
            };
          }
        };
      }
    };
  };
  return /* @__PURE__ */ $function($function$2($result$1));
}
