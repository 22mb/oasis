/**
 * Whether a `Cookie` header that a request sets reaches `fetch`. Browsers drop
 * it, since `Cookie` is a forbidden request header there: they send their own
 * cookies.
 */
export const cookieHeaders = !("document" in globalThis);
