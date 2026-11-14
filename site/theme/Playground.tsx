import { useEffect, useId, useRef, useState } from "react";
import exampleSpec from "../../tests/fixtures/petstore.yaml?raw";
import "./Playground.css";
import type { Reply, Request } from "./playground.worker.ts";

const t = {
  title: "Playground",
  lead: "Edit an OpenAPI 3.0, 3.1 or 3.2 document in JSON or YAML to see the TypeScript client that oasis-gen generates from it. The generator runs in your browser, so nothing you enter is sent anywhere. The page URL holds the document, so you can share it as a link. References to other files are not supported.",
  spec: "OpenAPI document",
  api: "api.ts (generated)",
  copy: "Copy link",
  copied: "Copied",
  copyFailed: "Could not copy",
  reset: "Reset",
  loading: "Loading…",
  loadFailed: "The playground could not be loaded.",
};

// The share link carries the document, if it differs from the example, as
// `#spec=…`: its UTF-8, compressed with raw DEFLATE and encoded as unpadded
// base64url.
async function encode(text: string): Promise<string> {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function decode(code: string): Promise<string> {
  const binary = atob(code.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Response(stream).text();
}

async function fromHash(): Promise<string> {
  const code = new URLSearchParams(location.hash.slice(1)).get("spec");
  if (code === null) return exampleSpec;
  try {
    return await decode(code);
  } catch {
    // A truncated or edited link: keep the example.
    return exampleSpec;
  }
}

/**
 * Puts `text` in the page URL, unless it is no longer the playground's text by
 * the time it is encoded: `current` gives that text, or `null` once the
 * playground is gone.
 */
async function updateHash(text: string, current: () => string | null): Promise<void> {
  const hash = text === exampleSpec ? "" : `#${new URLSearchParams({ spec: await encode(text) })}`;
  // A later edit schedules its own update, and another page has its own URL.
  if (text !== current()) return;
  history.replaceState(history.state, "", location.pathname + location.search + hash);
}

export function Playground() {
  const specId = useId();
  const apiId = useId();

  // `null` until the generator has loaded and read the document from the URL.
  const [spec, setSpec] = useState<string | null>(null);
  const [api, setApi] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const [error, setError] = useState("");
  const [copyStatus, setCopyStatus] = useState("");

  const worker = useRef<Worker | null>(null);
  // The newest request; replies to older ones are dropped.
  const requested = useRef(0);
  const latest = useRef("");
  const started = useRef(false);
  const mounted = useRef(false);
  const current = () => (mounted.current ? latest.current : null);
  const hashTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const copyTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    mounted.current = true;
    const fail = (reason: unknown) => {
      setFailed(true);
      setError(reason ? `${t.loadFailed}\n${reason}` : t.loadFailed);
    };
    let w: Worker;
    try {
      w = new Worker(new URL("./playground.worker.ts", import.meta.url), { type: "module" });
    } catch (e) {
      // For example, a content security policy that forbids workers.
      return fail(e);
    }
    worker.current = w;
    let unmounted = false;
    const ready = new Promise<void>((resolve, reject) => {
      // The event of a script that failed to load has no message.
      w.addEventListener("error", (e) => reject(e.message));
      w.addEventListener("message", ({ data }: MessageEvent<Reply>) => {
        if (data.type === "ready") resolve();
        else if (data.type === "failed") reject(data.message);
        else if (data.id === requested.current) {
          setError(data.ok ? "" : data.text);
          if (data.ok) setApi(data.text);
          setLoaded(true);
        }
      });
    });
    Promise.all([fromHash(), ready]).then(
      ([text]) => unmounted || setSpec(text),
      (e) => unmounted || fail(e),
    );
    return () => {
      unmounted = true;
      mounted.current = false;
      w.terminate();
      clearTimeout(hashTimer.current);
      clearTimeout(copyTimer.current);
    };
  }, []);

  // Regenerates once typing pauses; the first document is generated at once.
  useEffect(() => {
    if (spec === null) return;
    latest.current = spec;
    const generate = () => {
      const request: Request = { id: ++requested.current, text: spec };
      worker.current?.postMessage(request);
    };
    if (!started.current) {
      started.current = true;
      return generate();
    }
    const timer = setTimeout(generate, 300);
    clearTimeout(hashTimer.current);
    hashTimer.current = setTimeout(() => updateHash(spec, current), 300);
    return () => clearTimeout(timer);
  }, [spec]);

  async function copyLink() {
    try {
      // The link must hold the document as it is now, not as of the last update.
      clearTimeout(hashTimer.current);
      await updateHash(latest.current, current);
      if (!mounted.current) return;
      await navigator.clipboard.writeText(location.href);
      setCopyStatus(t.copied);
    } catch {
      setCopyStatus(t.copyFailed);
    }
    if (!mounted.current) return;
    clearTimeout(copyTimer.current);
    copyTimer.current = setTimeout(() => setCopyStatus(""), 2000);
  }

  return (
    <main className="playground rp-not-doc">
      <h1>{t.title}</h1>
      <p className="lead">{t.lead}</p>
      <div className="actions">
        {!loaded && !failed && <span className="loading">{t.loading}</span>}
        {/* Outside the button: a live region inside one is not announced reliably. */}
        <span className="copy-status" role="status">
          {copyStatus}
        </span>
        <button type="button" disabled={!loaded} onClick={() => setSpec(exampleSpec)}>
          {t.reset}
        </button>
        <button type="button" disabled={!loaded} onClick={copyLink}>
          {t.copy}
        </button>
      </div>
      <div className="panes">
        <section className="pane">
          <label className="bar" htmlFor={specId}>
            {t.spec}
          </label>
          <textarea
            id={specId}
            className="code"
            value={spec ?? ""}
            onChange={(e) => setSpec(e.target.value)}
            disabled={!loaded}
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
            autoCorrect="off"
            wrap="off"
          />
        </section>
        <section className="pane">
          <h2 id={apiId} className="bar">
            {t.api}
          </h2>
          {/* Always present, so that a message put into it is announced; polite,
              since a document is invalid most of the time while it is edited. */}
          <div className="problem" role="status">
            {error && <pre className="error">{error}</pre>}
          </div>
          {/* Shiki escapes the generated text. */}
          <div
            className="code"
            role="region"
            aria-labelledby={apiId}
            tabIndex={0}
            hidden={error !== ""}
            dangerouslySetInnerHTML={{ __html: api }}
          />
        </section>
      </div>
    </main>
  );
}
