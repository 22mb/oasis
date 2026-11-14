## Example

```sh
npx oasis-gen openapi.yaml -o src/api.ts
```

```ts twoslash
import { createClient } from "./api.ts";

const api = createClient({ baseUrl: "https://api.example.com" });

const res = await api.GET("/pets/{petId}", { path: { petId: 1 } });
if (res.status === 200) {
  res.data.name; // typed from the 200 response
} else if (res.status === 404) {
  res.data.title; // typed from the 404 response
}
```
