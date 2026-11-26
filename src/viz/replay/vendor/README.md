# Vendored player libraries

The replay player (`../player.html`) inlines these files into every generated `replay.html`,
so replays open offline from `file://` with no network access.

| File | Package | License |
|---|---|---|
| `d3.min.js` | [d3](https://www.npmjs.com/package/d3) 7.9.0, `dist/d3.min.js` | ISC (`LICENSE-d3`) |
| `d3-graphviz.min.js` | [d3-graphviz](https://www.npmjs.com/package/d3-graphviz) 5.6.0, `build/d3-graphviz.min.js` (bundles Graphviz compiled to WebAssembly via `@hpcc-js/wasm`) | BSD-3-Clause (`LICENSE-d3-graphviz`) |

SHA-256:

```
f2094bbf6141b359722c4fe454eb6c4b0f0e42cc10cc7af921fc158fceb86539  d3.min.js
9c1ecd146ca7f2ff57c849cee4398f58cee7a7a8c7d79c24b817bbaf4dd4e05c  d3-graphviz.min.js
```

## Updating

`npm pack` verifies the tarballs against the registry's integrity hashes:

```sh
cd "$(mktemp -d)"
npm pack d3@<version> d3-graphviz@<version>
tar -xzf d3-<version>.tgz && cp package/dist/d3.min.js package/LICENSE <this dir>/  # rename LICENSE
tar -xzf d3-graphviz-<version>.tgz && cp package/build/d3-graphviz.min.js package/LICENSE <this dir>/
```

d3-graphviz must stay compatible with the d3 major version (it peer-depends on
`d3-selection`). Update the table and hashes above, then regenerate a replay and check it renders.
