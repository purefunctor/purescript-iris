import * as Iris_Effect_Json from "../Iris.Effect.Json/index.js";
export const NotFound = ($value0) => ({
  tag: "NotFound",
  _1: $value0
});
export const BadRequest = ($value0) => ({
  tag: "BadRequest",
  _1: $value0
});
export const OutOfStock = ($value0) => ({
  tag: "OutOfStock",
  _1: $value0
});
export const UpstreamFailed = ($value0) => ({
  tag: "UpstreamFailed",
  _1: $value0
});
export function status(section47) {
  if (section47.tag === "NotFound") {
    return 404 | 0;
  }
  if (section47.tag === "BadRequest") {
    return 400 | 0;
  }
  if (section47.tag === "OutOfStock") {
    return 409 | 0;
  }
  if (section47.tag === "UpstreamFailed") {
    return 502 | 0;
  }
  throw new Error("Pattern match failure");
}
export function toJson(section86) {
  if (section86.tag === "NotFound") {
    const { _1: what } = section86;
    return message("not_found")(what + " not found");
  }
  if (section86.tag === "BadRequest") {
    const { _1: reason } = section86;
    return message("bad_request")(reason);
  }
  if (section86.tag === "OutOfStock") {
    const available = section86._1.available;
    const product = section86._1.product;
    const requested = section86._1.requested;
    return Iris_Effect_Json.fromObject([
      {
        name: "error",
        value: Iris_Effect_Json.fromString("out_of_stock")
      },
      {
        name: "product",
        value: Iris_Effect_Json.fromString(product)
      },
      {
        name: "requested",
        value: Iris_Effect_Json.fromInt(requested)
      },
      {
        name: "available",
        value: Iris_Effect_Json.fromInt(available)
      }
    ]);
  }
  if (section86.tag === "UpstreamFailed") {
    const { _1: reason$1 } = section86;
    return message("upstream_failed")(reason$1);
  }
  throw new Error("Pattern match failure");
}
function message(kind) {
  return (text) => {
    return Iris_Effect_Json.fromObject([{
      name: "error",
      value: Iris_Effect_Json.fromString(kind)
    }, {
      name: "message",
      value: Iris_Effect_Json.fromString(text)
    }]);
  };
}
