let log = [];
let observations = 0;

export const observe = choice => {
  observations += 1;
  log.push(`observe ${observations}`);
  return choice;
};

export const record = label => () => {
  log.push(label);
};

export const takeLog = () => {
  const taken = log;
  log = [];
  return taken;
};
