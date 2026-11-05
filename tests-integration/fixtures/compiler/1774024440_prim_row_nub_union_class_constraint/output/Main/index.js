import * as $foreign from "./foreign.js";
export function merge(unionR1R2R3Dict) {
  return (nubR3R4Dict) => {
    return ($record) => ($record$1) => unsafeCoerce({});
  };
}
export function collect(dictionary) {
  return dictionary.collect;
}
export function combine(collectItemsDict) {
  return (unionRowPartialRowDict) => {
    return (nubRowDedupedDict) => {
      return (unionDedupedDeduped_RequiredDict) => {
        return (provided) => {
          return ($items) => {
            const merged = /* @__PURE__ */ merge(unionRowPartialRowDict)(nubRowDedupedDict)({ key: "value" })(provided);
            return /* @__PURE__ */ consume(unionDedupedDeduped_RequiredDict)(merged);
          };
        };
      };
    };
  };
}
function combine_(collectItemsDict) {
  return /* @__PURE__ */ combine(collectItemsDict)({})({})({})({});
}
export const unsafeCoerce = $foreign["unsafeCoerce"];
export const consume = $foreign["consume"];
export const collectArrayString = { collect: (xs) => xs };
export const repro = /* @__PURE__ */ combine(collectArrayString)({})({})({})({})([]);
export { combine_ as "combine'" };
