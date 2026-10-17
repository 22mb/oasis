// Exact type equality for type-level tests. Assignability alone misses types
// that widen (`string | null` becoming `unknown`, a required key becoming optional).

/** `true` only when `A` and `B` are the same type. */
export type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;

/** Fails to compile unless `T` is `true`. */
export type Expect<T extends true> = T;
