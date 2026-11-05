import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Array from "../Data.Array/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_String_Common from "../Data.String.Common/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Console from "../Iris.Effect.Console/index.js";
import * as Iris_Effect_Http from "../Iris.Effect.Http/index.js";
import * as Iris_Effect_Http_Node from "../Iris.Effect.Http.Node/index.js";
import * as Iris_Effect_Json from "../Iris.Effect.Json/index.js";
import * as Signal from "../Signal/index.js";
import * as Test_HttpServer from "../Test.HttpServer/index.js";
import * as $foreign from "./foreign.js";
const MissingName = "MissingName";
const UnknownRoute = ($value0) => ({
  tag: "UnknownRoute",
  _1: $value0
});
const ServerFailed = ($value0) => ({
  tag: "ServerFailed",
  _1: $value0
});
const ClientFailed = ($value0) => ({
  tag: "ClientFailed",
  _1: $value0
});
function text(status) {
  return (body) => {
    return {
      status,
      headers: [{
        name: "content-type",
        value: "text/plain"
      }],
      body
    };
  };
}
function onError(section213) {
  if (section213 === "MissingName") {
    return text(400 | 0)("missing name");
  }
  if (section213.tag === "UnknownRoute") {
    const { _1: path } = section213;
    return text(404 | 0)("unknown route " + path);
  }
  throw new Error("Pattern match failure");
}
function header(name) {
  return (headers) => {
    return /* @__PURE__ */ functorMaybeDictMap((section261) => section261.value)(Data_Array.find((entry) => entry.name === name)(headers));
  };
}
function greeting(request) {
  const $scrutinee = Iris_Effect_Json.parse(request.body);
  if ($scrutinee.tag === "Right") {
    const { _1: json } = $scrutinee;
    const $scrutinee$1 = /* @__PURE__ */ Control_Bind.bind(Data_Maybe.bindMaybe)(Iris_Effect_Json.field("name")(json))(Iris_Effect_Json.toString);
    if ($scrutinee$1.tag === "Just") {
      const { _1: name } = $scrutinee$1;
      const asyncArgument = {
        status: 200 | 0,
        headers: [{
          name: "content-type",
          value: "application/json"
        }],
        body: Iris_Effect_Json.stringify(Iris_Effect_Json.fromObject([
          {
            name: "greeting",
            value: Iris_Effect_Json.fromString("Hello, " + (name + "!"))
          },
          {
            name: "query",
            value: Iris_Effect_Json.fromString(request.query)
          },
          {
            name: "contentType",
            value: Data_Maybe.fromMaybe(Iris_Effect_Json.null)(/* @__PURE__ */ functorMaybeDictMap(Iris_Effect_Json.fromString)(header("content-type")(request.headers)))
          }
        ]))
      };
      return (environment) => ({
        tag: "Pure",
        _1: asyncArgument
      });
    }
  }
  return (environment$1) => ({
    tag: "Throw",
    _1: "MissingName"
  });
}
function handler(services) {
  return (request) => {
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const probe = services["Main.Probe"](environment);
          const $scrutinee = request.method;
          const $scrutinee$1 = request.path;
          if ($scrutinee === "POST" && $scrutinee$1 === "/greet") {
            return greeting(request)(environment);
          }
          if ($scrutinee === "GET" && $scrutinee$1 === "/crash") {
            const asyncArgument = defect("handler crashed");
            const $unit = asyncArgument();
            const asyncArgument$1 = text(200 | 0)("unreachable");
            const $result = asyncArgument$1;
            return {
              tag: "Pure",
              _1: $result
            };
          }
          if ($scrutinee === "GET" && $scrutinee$1 === "/slow") {
            const $function = Iris_Effect_Async.finally;
            const asyncArgument$2 = Signal.fire(probe.released);
            const $result$1 = (environment$1) => {
              return {
                tag: "Run",
                _1: () => {
                  const value = asyncArgument$2();
                  return {
                    tag: "Pure",
                    _1: value
                  };
                }
              };
            };
            const $function$1 = $function($result$1);
            const asyncArgument$3 = Signal.fire(probe.started);
            const $result$2 = (environment$2) => {
              return {
                tag: "Run",
                _1: () => {
                  const $unit$1 = asyncArgument$3();
                  return Iris_Effect_Async.async(($function$2) => {
                    const $value = (environment$3) => ({
                      tag: "Pure",
                      _1: Data_Unit.unit
                    });
                    return () => {
                      return $value;
                    };
                  })(environment$2);
                }
              };
            };
            return $function$1($result$2)(environment);
          }
          const path = $scrutinee$1;
          const asyncArgument$4 = {
            tag: "UnknownRoute",
            _1: path
          };
          return {
            tag: "Throw",
            _1: asyncArgument$4
          };
        }
      };
    };
  };
}
function describe(services) {
  return (request) => {
    const asyncArgument = /* @__PURE__ */ Iris_Effect_Http.request(services)(request);
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: {
              tag: "Catch",
              _1: asyncArgument(environment),
              _2: (caughtError) => ({
                tag: "Throw",
                _1: {
                  tag: "ClientFailed",
                  _1: caughtError
                }
              })
            },
            _2: (response) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$1 = /* @__PURE__ */ Iris_Effect_Console.log(services)(Data_String_Common.joinWith(" ")([
                    request.method,
                    request.url,
                    "->",
                    /* @__PURE__ */ showIntDictShow(response.status),
                    Data_Maybe.fromMaybe("-")(header("content-type")(response.headers)),
                    response.body
                  ]));
                  const $action = asyncArgument$1(environment);
                  const value = $action();
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
  };
}
function program(services) {
  return (environment) => {
    return {
      tag: "Run",
      _1: () => {
        const probe = services["Main.Probe"](environment);
        const asyncArgument = /* @__PURE__ */ Test_HttpServer.serve(services)({
          host: "127.0.0.1",
          port: 0 | 0,
          onError
        })(/* @__PURE__ */ handler(services));
        return {
          tag: "Bind",
          _1: {
            tag: "Catch",
            _1: asyncArgument(environment),
            _2: (caughtError) => ({
              tag: "Throw",
              _1: {
                tag: "ServerFailed",
                _1: caughtError
              }
            })
          },
          _2: (server) => {
            return {
              tag: "Run",
              _1: () => {
                const listening = Test_HttpServer.address(server);
                const origin = "http://" + (listening.host + (":" + /* @__PURE__ */ showIntDictShow(listening.port)));
                const get = (path) => {
                  return {
                    method: "GET",
                    url: origin + path,
                    headers: [],
                    body: "Nothing"
                  };
                };
                const asyncArgument$1 = /* @__PURE__ */ describe(services)({
                  method: "POST",
                  url: origin + "/greet?from=test",
                  headers: [{
                    name: "content-type",
                    value: "application/json"
                  }],
                  body: {
                    tag: "Just",
                    _1: "{\"name\":\"Alice\"}"
                  }
                });
                return {
                  tag: "Bind",
                  _1: asyncArgument$1(environment),
                  _2: ($unit) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const asyncArgument$2 = /* @__PURE__ */ describe(services)({
                          method: "POST",
                          url: origin + "/greet",
                          headers: [],
                          body: {
                            tag: "Just",
                            _1: "{}"
                          }
                        });
                        return {
                          tag: "Bind",
                          _1: asyncArgument$2(environment),
                          _2: ($unit$1) => {
                            return {
                              tag: "Run",
                              _1: () => {
                                const asyncArgument$3 = /* @__PURE__ */ describe(services)(get("/missing"));
                                return {
                                  tag: "Bind",
                                  _1: asyncArgument$3(environment),
                                  _2: ($unit$2) => {
                                    return {
                                      tag: "Run",
                                      _1: () => {
                                        const asyncArgument$4 = /* @__PURE__ */ describe(services)(get("/crash"));
                                        return {
                                          tag: "Bind",
                                          _1: asyncArgument$4(environment),
                                          _2: ($unit$3) => {
                                            return {
                                              tag: "Run",
                                              _1: () => {
                                                const asyncArgument$5 = /* @__PURE__ */ describe(services)(get("/missing"));
                                                return {
                                                  tag: "Bind",
                                                  _1: asyncArgument$5(environment),
                                                  _2: ($unit$4) => {
                                                    return {
                                                      tag: "Run",
                                                      _1: () => {
                                                        const asyncArgument$6 = Iris_Effect_Async.fork(/* @__PURE__ */ Iris_Effect_Http.get(services)(origin + "/slow"));
                                                        return {
                                                          tag: "Bind",
                                                          _1: asyncArgument$6(environment),
                                                          _2: (pending) => {
                                                            return {
                                                              tag: "Run",
                                                              _1: () => {
                                                                const asyncArgument$7 = /* @__PURE__ */ Iris_Effect_Console.log(services)("slow request sent");
                                                                const $action = asyncArgument$7(environment);
                                                                const $unit$5 = $action();
                                                                const asyncArgument$8 = Signal.wait(probe.started);
                                                                return {
                                                                  tag: "Bind",
                                                                  _1: asyncArgument$8(environment),
                                                                  _2: ($unit$6) => {
                                                                    return {
                                                                      tag: "Run",
                                                                      _1: () => {
                                                                        const asyncArgument$9 = Iris_Effect_Async.kill({
                                                                          tag: "NetworkError",
                                                                          _1: "cancelled"
                                                                        })(pending);
                                                                        return {
                                                                          tag: "Bind",
                                                                          _1: {
                                                                            tag: "Catch",
                                                                            _1: asyncArgument$9(environment),
                                                                            _2: (caughtError$1) => ({
                                                                              tag: "Throw",
                                                                              _1: {
                                                                                tag: "ClientFailed",
                                                                                _1: caughtError$1
                                                                              }
                                                                            })
                                                                          },
                                                                          _2: ($unit$7) => {
                                                                            return {
                                                                              tag: "Run",
                                                                              _1: () => {
                                                                                const asyncArgument$10 = /* @__PURE__ */ Iris_Effect_Console.log(services)("slow request cancelled");
                                                                                const $action$1 = asyncArgument$10(environment);
                                                                                const $unit$8 = $action$1();
                                                                                const asyncArgument$11 = Signal.wait(probe.released);
                                                                                return {
                                                                                  tag: "Bind",
                                                                                  _1: asyncArgument$11(environment),
                                                                                  _2: ($unit$9) => {
                                                                                    return {
                                                                                      tag: "Run",
                                                                                      _1: () => {
                                                                                        const asyncArgument$12 = /* @__PURE__ */ Iris_Effect_Console.log(services)("slow handler released");
                                                                                        const $action$2 = asyncArgument$12(environment);
                                                                                        const $unit$10 = $action$2();
                                                                                        const asyncArgument$13 = /* @__PURE__ */ Test_HttpServer.serve(services)({
                                                                                          host: "127.0.0.1",
                                                                                          port: listening.port,
                                                                                          onError
                                                                                        })(/* @__PURE__ */ handler(services));
                                                                                        return {
                                                                                          tag: "Bind",
                                                                                          _1: {
                                                                                            tag: "Catch",
                                                                                            _1: {
                                                                                              tag: "Bind",
                                                                                              _1: asyncArgument$13(environment),
                                                                                              _2: (attemptValue) => ({
                                                                                                tag: "Pure",
                                                                                                _1: {
                                                                                                  tag: "Right",
                                                                                                  _1: attemptValue
                                                                                                }
                                                                                              })
                                                                                            },
                                                                                            _2: (caughtError$2) => ({
                                                                                              tag: "Pure",
                                                                                              _1: {
                                                                                                tag: "Left",
                                                                                                _1: caughtError$2
                                                                                              }
                                                                                            })
                                                                                          },
                                                                                          _2: (second) => {
                                                                                            return {
                                                                                              tag: "Run",
                                                                                              _1: () => {
                                                                                                const $function = Iris_Effect_Console.log(services);
                                                                                                let $result;
                                                                                                $case: {
                                                                                                  if (second.tag === "Left") {
                                                                                                    const { _1: error } = second;
                                                                                                    $result = "second listen rejected " + /* @__PURE__ */ showServerErrorDictShow(error);
                                                                                                    break $case;
                                                                                                  }
                                                                                                  if (second.tag === "Right") {
                                                                                                    $result = "second listen accepted";
                                                                                                    break $case;
                                                                                                  }
                                                                                                  throw new Error("Pattern match failure");
                                                                                                }
                                                                                                const asyncArgument$14 = /* @__PURE__ */ $function($result);
                                                                                                const $action$3 = asyncArgument$14(environment);
                                                                                                const $unit$11 = $action$3();
                                                                                                const asyncArgument$15 = Test_HttpServer.close(server);
                                                                                                return {
                                                                                                  tag: "Bind",
                                                                                                  _1: asyncArgument$15(environment),
                                                                                                  _2: ($unit$12) => {
                                                                                                    return {
                                                                                                      tag: "Run",
                                                                                                      _1: () => {
                                                                                                        const asyncArgument$16 = /* @__PURE__ */ Iris_Effect_Console.log(services)("server closed");
                                                                                                        const $action$4 = asyncArgument$16(environment);
                                                                                                        const value = $action$4();
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
                                                                                          }
                                                                                        };
                                                                                      }
                                                                                    };
                                                                                  }
                                                                                };
                                                                              }
                                                                            };
                                                                          }
                                                                        };
                                                                      }
                                                                    };
                                                                  }
                                                                };
                                                              }
                                                            };
                                                          }
                                                        };
                                                      }
                                                    };
                                                  }
                                                };
                                              }
                                            };
                                          }
                                        };
                                      }
                                    };
                                  }
                                };
                              }
                            };
                          }
                        };
                      }
                    };
                  }
                };
              }
            };
          }
        };
      }
    };
  };
}
const defect = $foreign["defect"];
const showServerErrorDictShow = /* @__PURE__ */ Data_Show.show(Test_HttpServer.showServerError);
const functorMaybeDictMap = /* @__PURE__ */ Data_Functor.map(Data_Maybe.functorMaybe);
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
export const showProgramError = /* @__PURE__ */ (() => {
  return { show: (section146) => {
    if (section146.tag === "ServerFailed") {
      const { _1: error } = section146;
      return "(ServerFailed " + (/* @__PURE__ */ showServerErrorDictShow(error) + ")");
    }
    if (section146.tag === "ClientFailed") {
      const { _1: error$1 } = section146;
      return "(ClientFailed " + (/* @__PURE__ */ Data_Show.show(Iris_Effect_Http.showHttpError)(error$1) + ")");
    }
    throw new Error("Pattern match failure");
  } };
})();
export const run = /* @__PURE__ */ (() => {
  const $function = (section1006) => (section1008) => ({
    started: section1006,
    released: section1008
  });
  const $action = Signal.new;
  const $argumentAction = Signal.new;
  return () => {
    let probe;
    let $function$1;
    $function$1 = $function($action());
    probe = $function$1($argumentAction());
    const environment = {
      server: Test_HttpServer.default,
      http: Iris_Effect_Http_Node.client,
      console: Iris_Effect_Console.default,
      probe
    };
    return (/* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => /* @__PURE__ */ Iris_Effect_Console.error({ "Iris.Effect.Console.Console": (environment$1) => environment$1.console })(/* @__PURE__ */ Data_Show.show(showProgramError)(error))(environment) })(environment)(/* @__PURE__ */ program({
      "Test.HttpServer.HttpServer": (environment$2) => environment$2.server,
      "Iris.Effect.Http.Http": (environment$3) => environment$3.http,
      "Iris.Effect.Console.Console": (environment$4) => environment$4.console,
      "Main.Probe": (environment$5) => environment$5.probe
    }))))();
  };
})();
