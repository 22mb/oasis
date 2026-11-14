/**
 * Whether a `Cookie` header set on a request reaches `fetch`. In browsers,
 * `Cookie` is a forbidden request header, so they drop it and send their own
 * cookies.
 */
export const cookieHeaders = !("document" in globalThis);
