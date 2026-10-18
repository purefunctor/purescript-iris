import * as $foreign from "./foreign.js";
export const Unit = "Unit";
export const Spec = "Spec";
export const Token = "Token";
export function identity(value) {
  return value;
}
export function modify(token) {
  return token;
}
export const mkEval = $foreign["mkEval"];
export const constrainedEval = $foreign["constrainedEval"];
export const consumeIdentity = $foreign["consumeIdentity"];
export const leak = $foreign["leak"];
export const witnessValue = {};
export const validLocalEval = /* @__PURE__ */ (() => {
  const $eval = mkEval("Spec");
  return consumeIdentity($eval);
})();
export const validNestedLocalEval = /* @__PURE__ */ (() => {
  const inner = mkEval("Spec");
  const outer = inner;
  return consumeIdentity(outer);
})();
export const escapedLocalEvidence = /* @__PURE__ */ (() => {
  const $eval = /* @__PURE__ */ constrainedEval(witnessValue)("Spec");
  return consumeIdentity($eval);
})();
export const escapedLocalAlias = /* @__PURE__ */ (() => {
  const escaped = leak(modify);
  return escaped;
})();
export const escapedEtaExpansion = leak((token) => modify(token));
export const escapedIdentityWrapper = identity(leak(modify));
export const escapedArrayWrapper = [leak(modify)];
export const escapedConditionalWrapper = leak((token) => true ? modify(token) : modify(token));
export const escapedUnusedLocal = /* @__PURE__ */ (() => {
  const escaped = leak(modify);
  return 0 | 0;
})();
