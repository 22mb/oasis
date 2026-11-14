declare module "*?raw" {
  const text: string;
  export default text;
}

declare module "*.css";

declare module "*.md" {
  const Content: (props: { components?: Record<string, unknown> }) => React.JSX.Element;
  export default Content;
}
