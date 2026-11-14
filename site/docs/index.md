---
pageType: home

hero:
  name: oasis-gen
  text: A strictly typed API client generator for OpenAPI, with zero runtime dependencies
  tagline: One dependency-free TypeScript file, with types for every operation and a small fetch-based runtime.
  image:
    src: /logo.svg
    alt: ""
  actions:
    - theme: brand
      text: Get started
      link: /getting-started
    - theme: alt
      text: GitHub
      link: https://github.com/22mb/oasis

features:
  - icon: /icons/strict.svg
    title: Strict types
    details: <code>status</code> narrows <code>data</code> to the declared response. Statuses that the document does not declare stay in the type. Request and response types respect <code>readOnly</code> and <code>writeOnly</code>.
  - icon: /icons/file.svg
    title: Zero runtime dependencies
    details: The output is a single TypeScript file that runs anywhere fetch is available. There is no runtime package to install and nothing to configure.
  - icon: /icons/spec.svg
    title: OpenAPI 3.0 to 3.2
    details: oasis-gen reads OpenAPI 3.0, 3.1 and 3.2 documents, including QUERY operations, sequential media types, multi-file documents, and JSON Schema 2020-12.
---
