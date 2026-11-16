export const foldrArray = (f) => (z) => (xs) => xs.reduceRight((acc, x) => f(x)(acc), z);
