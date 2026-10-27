export const queue = [];

export const hasNext = () => queue.length > 0;

export const next = () => {
  queue.shift();
};
