import { HomeLayout as BasicHomeLayout, getCustomMDXComponent } from "@rspress/core/theme-original";
// Not a page of its own: the name starts with `_`.
import Example from "../docs/_example.md";
import "./index.css";

/** The home page, with a first example under its features. */
function HomeLayout() {
  return (
    <BasicHomeLayout
      // The hero's title is not a heading.
      beforeHero={<h1 className="visually-hidden">oasis-gen</h1>}
      afterFeatures={
        <div className="home-example rp-doc">
          <Example components={getCustomMDXComponent()} />
        </div>
      }
    />
  );
}

export { HomeLayout };
export * from "@rspress/core/theme-original";
