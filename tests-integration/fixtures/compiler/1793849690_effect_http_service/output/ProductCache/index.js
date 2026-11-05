import * as Data_Boolean from "../Data.Boolean/index.js";
import * as Data_Map_Internal from "../Data.Map.Internal/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
export function make($record) {
  const capacity = $record.capacity;
  const $value = {
    products: Data_Map_Internal.empty,
    arrivals: Data_Map_Internal.empty,
    next: 0 | 0
  };
  return () => {
    const entries = { value: $value };
    return {
      capacity,
      entries
    };
  };
}
export function lookup(services) {
  return (identifier) => {
    return (environment) => {
      const $value = services["ProductCache.ProductCache"](environment);
      return () => {
        const $reference = $value.entries;
        const syncArgument = () => {
          return $reference.value;
        };
        const entries = syncArgument();
        const syncArgument$1 = /* @__PURE__ */ Data_Map_Internal.lookup(Data_Ord.ordString)(identifier)(entries.products);
        return syncArgument$1;
      };
    };
  };
}
export function insert(services) {
  return (product) => {
    return (environment) => {
      const $value = services["ProductCache.ProductCache"](environment);
      return () => {
        const cache = $value;
        const modifyFunction = store(cache.capacity)(product);
        const modifyReference = cache.entries;
        const syncArgument = () => {
          const value = modifyReference.value;
          const $value$1 = modifyFunction(value);
          modifyReference.value = $value$1;
          return Data_Unit.unit;
        };
        return syncArgument();
      };
    };
  };
}
function store(capacity) {
  return (product) => {
    return (entries) => {
      if (/* @__PURE__ */ Data_Map_Internal.member(Data_Ord.ordString)(product.identifier)(entries.products)) {
        return {
          ...entries,
          products: /* @__PURE__ */ Data_Map_Internal.insert(Data_Ord.ordString)(product.identifier)(product)(entries.products)
        };
      }
      if (Data_Boolean.otherwise) {
        const admitted = evict(capacity)(entries);
        return {
          products: /* @__PURE__ */ Data_Map_Internal.insert(Data_Ord.ordString)(product.identifier)(product)(admitted.products),
          arrivals: /* @__PURE__ */ Data_Map_Internal.insert(Data_Ord.ordInt)(admitted.next)(product.identifier)(admitted.arrivals),
          next: admitted.next + (1 | 0) | 0
        };
      }
      throw new Error("Pattern match failure");
    };
  };
}
function evict(capacity) {
  return (entries) => {
    if (Data_Map_Internal.size(entries.products) < capacity) {
      return entries;
    }
    if (Data_Boolean.otherwise) {
      const $scrutinee = Data_Map_Internal.findMin(entries.arrivals);
      if ($scrutinee.tag === "Just") {
        const { _1: oldest } = $scrutinee;
        return {
          ...entries,
          products: /* @__PURE__ */ Data_Map_Internal.delete(Data_Ord.ordString)(oldest.value)(entries.products),
          arrivals: /* @__PURE__ */ Data_Map_Internal.delete(Data_Ord.ordInt)(oldest.key)(entries.arrivals)
        };
      }
      if ($scrutinee === "Nothing") {
        return entries;
      }
      throw new Error("Pattern match failure");
    }
    throw new Error("Pattern match failure");
  };
}
