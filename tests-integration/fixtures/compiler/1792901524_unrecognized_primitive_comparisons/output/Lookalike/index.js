export const Wrapped = ($value0) => ({
  tag: "Wrapped",
  _1: $value0
});
export function lessThan($int) {
  return ($int$1) => {
    return false;
  };
}
export const eqInt = { eq: ($wrapped) => ($wrapped$1) => true };
export const ordInt = {
  Eq0: () => eqInt,
  compare: ($wrapped) => ($wrapped$1) => "GT"
};
