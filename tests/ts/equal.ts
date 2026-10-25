// Exact type equality for type-level tests. Assignability alone misses types
// that widen (`string | null` becoming `unknown`, a required key becoming optional).

/** `true` only when `A` and `B` are the same type. */
export type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;

/** Fails to compile unless `T` is `true`. */
export type Expect<T extends true> = T;

/** A generated client's result, spelled out. */
export type Result<S, O, D> = { status: S; ok: O; data: D; response: Response };

// Status ranges, counted with tuples rather than the runtime's template literals.
type Below<N extends number, Acc extends unknown[] = [], Out = never> =
  Acc["length"] extends N ? Out : Below<N, [...Acc, unknown], Out | Acc["length"]>;
/** `From` up to, but excluding, `To`. */
export type Codes<From extends number, To extends number> = Exclude<Below<To>, Below<From>>;
export type Success = Codes<200, 300>;
export type Failure = Codes<300, 600>;
