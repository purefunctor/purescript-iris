export const defect = (message) => () => {
  throw new Error(message);
};
