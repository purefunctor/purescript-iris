import * as AppError from "../AppError/index.js";
import * as Catalog from "../Catalog/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Int from "../Data.Int/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Json from "../Iris.Effect.Json/index.js";
import * as OrderStore from "../OrderStore/index.js";
import * as ProductCache from "../ProductCache/index.js";
function fromCatalog(identifier) {
  return (section114) => {
    if (section114 === "Missing") {
      return {
        tag: "NotFound",
        _1: "product " + identifier
      };
    }
    if (section114.tag === "Unavailable") {
      const { _1: reason } = section114;
      return {
        tag: "UpstreamFailed",
        _1: reason
      };
    }
    if (section114 === "Abandoned") {
      return {
        tag: "UpstreamFailed",
        _1: "abandoned"
      };
    }
    throw new Error("Pattern match failure");
  };
}
export function getProduct(services) {
  return (identifier) => {
    const asyncArgument = /* @__PURE__ */ ProductCache.lookup(services)(identifier);
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          const $action = asyncArgument(environment);
          const cached = $action();
          if (cached.tag === "Just") {
            const { _1: product } = cached;
            return {
              tag: "Pure",
              _1: product
            };
          }
          if (cached === "Nothing") {
            const asyncArgument$1 = fromCatalog(identifier);
            const fetchListing = Iris_Effect_Async.fork(/* @__PURE__ */ Catalog.listing(services)(identifier));
            const asyncArgument$2 = Iris_Effect_Async.bracket(fetchListing)(Iris_Effect_Async.kill("Abandoned"))((pending) => {
              const asyncArgument$3 = /* @__PURE__ */ Catalog.stock(services)(identifier);
              return (environment$1) => {
                return {
                  tag: "Run",
                  _1: () => {
                    return {
                      tag: "Bind",
                      _1: asyncArgument$3(environment$1),
                      _2: (available) => {
                        return {
                          tag: "Run",
                          _1: () => {
                            const asyncArgument$4 = Iris_Effect_Async.join(pending);
                            return {
                              tag: "Bind",
                              _1: asyncArgument$4(environment$1),
                              _2: (listing) => ({
                                tag: "Pure",
                                _1: {
                                  listing,
                                  available
                                }
                              })
                            };
                          }
                        };
                      }
                    };
                  }
                };
              };
            });
            return {
              tag: "Bind",
              _1: {
                tag: "Catch",
                _1: asyncArgument$2(environment),
                _2: (caughtError) => ({
                  tag: "Throw",
                  _1: asyncArgument$1(caughtError)
                })
              },
              _2: (bindValue) => {
                return (($record) => {
                  const listing$1 = $record.listing;
                  const available$1 = $record.available;
                  const product$1 = {
                    identifier,
                    name: listing$1.name,
                    price: listing$1.price,
                    stock: available$1
                  };
                  const asyncArgument$5 = /* @__PURE__ */ ProductCache.insert(services)(product$1);
                  return (environment$2) => {
                    return {
                      tag: "Run",
                      _1: () => {
                        const $action$1 = asyncArgument$5(environment$2);
                        const $unit = $action$1();
                        return {
                          tag: "Pure",
                          _1: product$1
                        };
                      }
                    };
                  };
                })(bindValue)(environment);
              }
            };
          }
          throw new Error("Pattern match failure");
        }
      };
    };
  };
}
export function createOrder(services) {
  return (body) => {
    const asyncArgument = either(AppError.BadRequest)(parseOrder(body));
    return (environment) => {
      return {
        tag: "Run",
        _1: () => {
          return {
            tag: "Bind",
            _1: asyncArgument(environment),
            _2: (request) => {
              return {
                tag: "Run",
                _1: () => {
                  const asyncArgument$1 = /* @__PURE__ */ getProduct(services)(request.product);
                  return {
                    tag: "Bind",
                    _1: asyncArgument$1(environment),
                    _2: (product) => {
                      return {
                        tag: "Run",
                        _1: () => {
                          const asyncArgument$2 = {
                            tag: "OutOfStock",
                            _1: {
                              product: product.identifier,
                              requested: request.quantity,
                              available: product.stock
                            }
                          };
                          if (product.stock < request.quantity) {
                            return {
                              tag: "Throw",
                              _1: asyncArgument$2
                            };
                          } else {
                            return (/* @__PURE__ */ (($unit) => {
                              const asyncArgument$3 = /* @__PURE__ */ OrderStore.create(services)({
                                product: product.identifier,
                                quantity: request.quantity,
                                total: product.price * request.quantity | 0
                              });
                              const $action = asyncArgument$3(environment);
                              return () => {
                                const value = $action();
                                return {
                                  tag: "Pure",
                                  _1: value
                                };
                              };
                            })(Data_Unit.unit))();
                          }
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
  };
}
function parseOrder(body) {
  return /* @__PURE__ */ bindEitherStringDictBind(Iris_Effect_Json.parse(body))((json) => /* @__PURE__ */ bindEitherStringDictBind(Data_Either.note("missing field product")(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("product")(json))(Iris_Effect_Json.toString)))((product) => /* @__PURE__ */ bindEitherStringDictBind(Data_Either.note("missing field quantity")(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("quantity")(json))(Iris_Effect_Json.toInt)))((quantity) => quantity > (0 | 0) ? {
    tag: "Right",
    _1: {
      product,
      quantity
    }
  } : {
    tag: "Left",
    _1: "quantity must be positive"
  })));
}
export function getOrder(services) {
  return (text) => {
    const $scrutinee = Data_Int.fromString(text);
    if ($scrutinee === "Nothing") {
      const asyncArgument = {
        tag: "BadRequest",
        _1: "invalid order number " + text
      };
      return (environment) => ({
        tag: "Throw",
        _1: asyncArgument
      });
    }
    if ($scrutinee.tag === "Just") {
      const { _1: number } = $scrutinee;
      const asyncArgument$1 = /* @__PURE__ */ OrderStore.find(services)(number);
      return (environment$1) => {
        return {
          tag: "Run",
          _1: () => {
            const $action = asyncArgument$1(environment$1);
            const found = $action();
            if (found.tag === "Just") {
              const { _1: order } = found;
              return {
                tag: "Pure",
                _1: order
              };
            }
            if (found === "Nothing") {
              return {
                tag: "Throw",
                _1: {
                  tag: "NotFound",
                  _1: "order " + text
                }
              };
            }
            throw new Error("Pattern match failure");
          }
        };
      };
    }
    throw new Error("Pattern match failure");
  };
}
function either(wrap) {
  return (section637) => {
    if (section637.tag === "Right") {
      const { _1: value } = section637;
      return (environment) => ({
        tag: "Pure",
        _1: value
      });
    }
    if (section637.tag === "Left") {
      const { _1: message } = section637;
      const asyncArgument = wrap(message);
      return (environment$1) => ({
        tag: "Throw",
        _1: asyncArgument
      });
    }
    throw new Error("Pattern match failure");
  };
}
export function encodeProduct(product) {
  return Iris_Effect_Json.fromObject([
    {
      name: "id",
      value: Iris_Effect_Json.fromString(product.identifier)
    },
    {
      name: "name",
      value: Iris_Effect_Json.fromString(product.name)
    },
    {
      name: "price",
      value: Iris_Effect_Json.fromInt(product.price)
    },
    {
      name: "stock",
      value: Iris_Effect_Json.fromInt(product.stock)
    }
  ]);
}
export function encodeOrder(order) {
  return Iris_Effect_Json.fromObject([
    {
      name: "number",
      value: Iris_Effect_Json.fromInt(order.number)
    },
    {
      name: "product",
      value: Iris_Effect_Json.fromString(order.product)
    },
    {
      name: "quantity",
      value: Iris_Effect_Json.fromInt(order.quantity)
    },
    {
      name: "total",
      value: Iris_Effect_Json.fromInt(order.total)
    }
  ]);
}
const bindEitherStringDictBind = /* @__PURE__ */ Control_Bind.bind(Data_Either.bindEither);
const bindMaybeDictBind = /* @__PURE__ */ Control_Bind.bind(Data_Maybe.bindMaybe);
