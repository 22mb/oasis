/** A request as `fetch` received it. */
export type Sent = { url: string; method: string; headers: Headers; body: string };

/** A fetch that records requests and answers with `responses` in turn (the last one repeats). */
export function server(...responses: (() => Response)[]) {
  const sent: Sent[] = [];
  const fetch = async (request: Request) => {
    sent.push({ url: request.url, method: request.method, headers: request.headers, body: await request.text() });
    return (responses[sent.length - 1] ?? responses.at(-1)!)();
  };
  return { fetch, sent };
}

export const noContent = () => new Response(null, { status: 204 });

export const json = (value: unknown) => () =>
  new Response(JSON.stringify(value), { headers: { "content-type": "application/json" } });
