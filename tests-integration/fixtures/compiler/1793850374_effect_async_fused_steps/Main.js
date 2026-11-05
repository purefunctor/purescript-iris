export const recorded = [];
export const completed = [];
export const errors = [];

export const record = (value) => () => {
  recorded.push(value);
  return value * 2;
};
export const later = (callback) => () => queueMicrotask(() => callback(17)());
export const complete = (value) => () => completed.push(value);
export const failed = (error) => () => errors.push(error);
