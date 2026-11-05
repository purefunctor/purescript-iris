export const segments = (path) => path.split("/").filter((segment) => segment !== "");

export const defect = (message) => () => {
  throw new Error(message);
};
