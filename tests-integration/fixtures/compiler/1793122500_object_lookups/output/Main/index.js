export function member(key) {
  return (object) => {
    return key in object;
  };
}
export function keys(object) {
  return Object.keys(object);
}
export function index(key) {
  return (object) => {
    return object[key];
  };
}
export function indexLiteral(object) {
  return object.a + object["b-c"] | 0;
}
