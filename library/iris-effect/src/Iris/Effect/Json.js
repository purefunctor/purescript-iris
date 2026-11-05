export const parseWith = (left) => (right) => (text) => {
  try {
    return right(JSON.parse(text));
  } catch (error) {
    return left(String(error.message));
  }
};

export const stringify = (json) => JSON.stringify(json);

const nullJson = null;

export { nullJson as null };

export const fromString = (value) => value;

export const fromNumber = (value) => value;

export const fromInt = (value) => value;

export const fromBoolean = (value) => value;

export const fromArray = (values) => values;

export const fromObject = (fields) => {
  const object = {};
  for (const entry of fields) {
    Object.defineProperty(object, entry.name, {
      value: entry.value,
      enumerable: true,
      writable: true,
      configurable: true,
    });
  }
  return object;
};

export const toStringWith = (just) => (nothing) => (json) =>
  typeof json === "string" ? just(json) : nothing;

export const toNumberWith = (just) => (nothing) => (json) =>
  typeof json === "number" ? just(json) : nothing;

export const toIntWith = (just) => (nothing) => (json) =>
  Number.isInteger(json) && json >= -2147483648 && json <= 2147483647 ? just(json | 0) : nothing;

export const toBooleanWith = (just) => (nothing) => (json) =>
  typeof json === "boolean" ? just(json) : nothing;

export const toArrayWith = (just) => (nothing) => (json) =>
  Array.isArray(json) ? just(json) : nothing;

export const fieldWith = (just) => (nothing) => (name) => (json) =>
  json !== null && typeof json === "object" && !Array.isArray(json) && Object.hasOwn(json, name)
    ? just(json[name])
    : nothing;
