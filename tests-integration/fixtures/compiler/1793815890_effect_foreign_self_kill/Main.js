export const later = (effect) => () => {
  setTimeout(effect, 1);
};
