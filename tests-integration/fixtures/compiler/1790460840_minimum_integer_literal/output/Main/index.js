export function test4($int) {
  if ($int === (-2147483648 | 0)) {
    return true;
  }
  return false;
}
export const test = -2147483648 | 0;
export const test2 = -2147483648 | 0;
export const test3 = -2147483648 | 0;
export const test5 = -2147483648 | 0;
export const test6 = -2147483647 | 0;
export const test7 = 2147483647 | 0;
export const test8 = ((value) => value)(42 | 0);
