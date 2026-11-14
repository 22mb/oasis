import type { ShikiTransformer } from "shiki";

type Root = Parameters<NonNullable<ShikiTransformer["root"]>>[0];
type Node = Root["children"][number];
type Element = Extract<Node, { type: "element" }>;

const isElement = (node: unknown): node is Element => (node as Node | undefined)?.type === "element";

const classes = (e: Element): unknown[] => [e.properties["class"] ?? e.properties["className"] ?? []].flat();

/** Collects the code in `node`, stopping if it reaches `stop`. */
function textBefore(node: Element["children"][number], stop: Element): [text: string, found: boolean] {
  if (node === stop) return ["", true];
  if (node.type === "text") return [node.value, false];
  // Popups on earlier tokens are not part of the code.
  if (!isElement(node) || node.tagName === "twoslash-popup-container") return ["", false];
  let text = "";
  for (const child of node.children) {
    const [t, found] = textBefore(child, stop);
    text += t;
    if (found) return [text, true];
  }
  return [text, false];
}

/** Finds a `^?` query popup in `line` and the token it is attached to, if any. */
function query(line: Element): { trigger: Element; popup: Element } | undefined {
  for (const child of line.children) {
    if (!isElement(child)) continue;
    if (child.tagName === "twoslash-popup-trigger") {
      const popup = child.children.find(
        (c): c is Element =>
          isElement(c) && c.tagName === "twoslash-popup-container" && c.properties["data-always"] === "true",
      );
      if (popup) return { trigger: child, popup };
    }
    const nested = query(child);
    if (nested) return nested;
  }
  return undefined;
}

function rewrite(parent: Root | Element) {
  for (let i = 0; i < parent.children.length; i++) {
    const line = parent.children[i];
    if (!isElement(line)) continue;
    if (!classes(line).includes("line")) {
      rewrite(line);
      continue;
    }
    const found = query(line);
    if (!found) continue;
    const { trigger, popup } = found;
    trigger.children = trigger.children.filter((c) => c !== popup);
    const result: Element = {
      type: "element",
      tagName: "div",
      // `rp-copy-ignore` prevents the copy button from copying the result.
      properties: { class: "twoslash-query-line rp-copy-ignore" },
      children: [
        // Align under the token the query points at.
        { type: "text", value: " ".repeat(textBefore(line, trigger)[0].length) },
        ...popup.children.filter((c) => isElement(c) && classes(c).includes("twoslash-popup-inner")),
      ],
    };
    // The block ends the line, so it replaces the following line feed.
    const next = parent.children[i + 1];
    const feed = next?.type === "text" && next.value === "\n" ? 1 : 0;
    parent.children.splice(i + 1, feed, result);
  }
}

/**
 * Shows the result of a `^?` query as a line below the code line it queries.
 * `@rspress/plugin-twoslash` renders it as an always-open popup drawn over
 * whatever follows, so a multi-line result covers code and text. This
 * transformer runs on the plugin's output because `root` is the last hook.
 */
export const twoslashQueriesAsLines: ShikiTransformer = {
  name: "oasis:twoslash-queries-as-lines",
  root: rewrite,
};
