let log = [];

export const record = label => () => {
  log.push(label);
};

export const takeLog = () => {
  const taken = log;
  log = [];
  return taken;
};
