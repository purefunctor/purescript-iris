function $function(left) {
  return ($int) => {
    return left;
  };
}
export function x(value) {
  return $function(value)(value);
}
export { $function as "function" };
