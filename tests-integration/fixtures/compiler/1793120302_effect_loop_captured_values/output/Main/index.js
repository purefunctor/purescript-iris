export function untilLimit(configuration) {
  return () => {
    let counter = 0 | 0;
    const limit = configuration.limit;
    while (true) {
      let $condition;
      let current;
      const section63 = counter;
      const $value = section63 + (1 | 0) | 0;
      current = counter = $value;
      $condition = current >= limit;
      if ($condition) {
        break;
      }
    }
    return counter;
  };
}
export function whileLimit(configuration) {
  return () => {
    let counter = 0 | 0;
    const limit = configuration.limit;
    while (true) {
      let $condition;
      const current = counter;
      $condition = current < limit;
      if (!$condition) {
        break;
      }
      const section147 = counter;
      const $value = section147 + (1 | 0) | 0;
      counter = $value;
    }
    return counter;
  };
}
