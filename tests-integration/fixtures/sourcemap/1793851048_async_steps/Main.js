export const record = (value) => () => value;
export const later = (callback) => () => callback(1)();
