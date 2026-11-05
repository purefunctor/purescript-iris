import * as Data_Map_Internal from "../Data.Map.Internal/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
export function create(services) {
  return ($record) => {
    const product = $record.product;
    const quantity = $record.quantity;
    const total = $record.total;
    return (environment) => {
      const $value = services["OrderStore.OrderStore"](environment);
      return () => {
        const store = $value;
        const syncArgument = () => {
          const state = store.value;
          const order = {
            number: state.next,
            product,
            quantity,
            total
          };
          const $value$1 = {
            orders: /* @__PURE__ */ Data_Map_Internal.insert(Data_Ord.ordInt)(order.number)(order)(state.orders),
            next: state.next + (1 | 0) | 0
          };
          store.value = $value$1;
          return order;
        };
        return syncArgument();
      };
    };
  };
}
export function find(services) {
  return (number) => {
    return (environment) => {
      const $value = services["OrderStore.OrderStore"](environment);
      return () => {
        const syncArgument = () => {
          return $value.value;
        };
        const state = syncArgument();
        const syncArgument$1 = /* @__PURE__ */ Data_Map_Internal.lookup(Data_Ord.ordInt)(number)(state.orders);
        return syncArgument$1;
      };
    };
  };
}
export const make = /* @__PURE__ */ (() => {
  const $function = (value) => value;
  const $value = {
    orders: Data_Map_Internal.empty,
    next: 1 | 0
  };
  return () => {
    return $function({ value: $value });
  };
})();
