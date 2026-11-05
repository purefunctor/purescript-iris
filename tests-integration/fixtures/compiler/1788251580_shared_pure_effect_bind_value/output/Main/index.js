export function sharedBind($unit) {
  return () => {
    const value = 42 | 0;
    return {
      left: value,
      right: value
    };
  };
}
