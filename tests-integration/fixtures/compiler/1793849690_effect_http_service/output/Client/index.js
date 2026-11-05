import * as Data_Show from "../Data.Show/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Http from "../Iris.Effect.Http/index.js";
function describe(services) {
  return (baseUrl) => {
    return ($record) => {
      const body = $record.body;
      const method = $record.method;
      const path = $record.path;
      const $function = Iris_Effect_Http.request(services);
      const $field = baseUrl + path;
      let $result;
      $case: {
        if (body.tag === "Just") {
          $result = [{
            name: "content-type",
            value: "application/json"
          }];
          break $case;
        }
        if (body === "Nothing") {
          $result = [];
          break $case;
        }
        throw new Error("Pattern match failure");
      }
      const asyncArgument = /* @__PURE__ */ $function({
        method,
        url: $field,
        headers: $result,
        body
      });
      return (environment) => {
        return {
          tag: "Run",
          _1: () => {
            return {
              tag: "Bind",
              _1: asyncArgument(environment),
              _2: (response) => {
                const asyncArgument$1 = method + (" " + (path + (" " + (/* @__PURE__ */ Data_Show.show(Data_Show.showInt)(response.status) + (" " + response.body)))));
                const $result$1 = asyncArgument$1;
                return {
                  tag: "Pure",
                  _1: $result$1
                };
              }
            };
          }
        };
      };
    };
  };
}
export function scenario(services) {
  return (baseUrl) => /* @__PURE__ */ Data_Traversable.traverse(Data_Traversable.traversableArray)(Iris_Effect_Async.applicativeAsync)(/* @__PURE__ */ describe(services)(baseUrl))([
    {
      method: "GET",
      path: "/health",
      body: "Nothing"
    },
    {
      method: "GET",
      path: "/products/p1",
      body: "Nothing"
    },
    {
      method: "POST",
      path: "/orders",
      body: {
        tag: "Just",
        _1: "{\"product\":\"p1\",\"quantity\":2}"
      }
    },
    {
      method: "GET",
      path: "/orders/1",
      body: "Nothing"
    },
    {
      method: "POST",
      path: "/orders",
      body: {
        tag: "Just",
        _1: "{\"product\":\"p1\",\"quantity\":5000}"
      }
    },
    {
      method: "POST",
      path: "/orders",
      body: {
        tag: "Just",
        _1: "not json"
      }
    },
    {
      method: "POST",
      path: "/orders",
      body: {
        tag: "Just",
        _1: "{\"product\":\"p1\",\"quantity\":0}"
      }
    },
    {
      method: "GET",
      path: "/products/missing",
      body: "Nothing"
    },
    {
      method: "GET",
      path: "/products/broken",
      body: "Nothing"
    },
    {
      method: "GET",
      path: "/orders/99",
      body: "Nothing"
    },
    {
      method: "GET",
      path: "/orders/first",
      body: "Nothing"
    },
    {
      method: "DELETE",
      path: "/orders",
      body: "Nothing"
    },
    {
      method: "GET",
      path: "/crash",
      body: "Nothing"
    },
    {
      method: "GET",
      path: "/health",
      body: "Nothing"
    }
  ]);
}
