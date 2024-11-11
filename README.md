# farm-plugin-resources-import-meta

This quick and dirty farm plugin replaces `import.meta.resources` with an array of generated resources.

4 in-place expression replacements are supported:

- `import.meta.resources` -> `Array<{name: string, type: string}> & { [resource_type: string]: Array<{name: string, type: string}> }`
- `import.meta.resources[n]` -> `{name: string, type: string}`
- `import.meta.resources.xxx` -> `Array<{name: string, type: string}>`
- `import.meta.resources.xxx[n]` -> `{name: string, type: string}`

`xxx` is a resource type defined by farm core or other plugin (i.e. `js`, `css`, `html`, `sourceMapJs`, `sourceMapCss`, etc.).

Order is not guaranteed.

Expressions are unmodified if the resource type is not found or index is out of bounds.

## Usage

```sh
npm i -D farm-plugin-resources-import-meta
```

```ts
// farm.config.ts

import { defineConfig } from "@farmfe/core";

export default defineConfig({
  plugins: [
    "farm-plugin-resources-import-meta",
  ],
});
```

Then anywhere in your code:

```js
console.log(import.meta.resources);
console.log(import.meta.resources.js);
console.log(import.meta.resources.css[0]);
```

May be replaced with

```js
console.log([{name:"main.js",type:"js"},{name:"main.css",type:"css"}]);
console.log([{name:"main.js",type:"js"}]);
console.log({name:"main.css",type:"css"});
```
