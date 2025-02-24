import {build, context} from 'esbuild';
import {isAbsolute, join, resolve} from 'path';
import {readFile} from 'fs/promises';

let wasmPlugin = {
    name: 'wasm',
    setup(build) {
      // Resolve ".wasm" files to a path with a namespace
      build.onResolve({ filter: /\.wasm$/ }, args => {
        if (args.resolveDir === '') {
          return // Ignore unresolvable paths
        }
        return {
          path: isAbsolute(args.path) ? args.path : join(args.resolveDir, args.path),
            namespace: 'wasm-binary',
        }
      }),

      // Virtual modules in the "wasm-binary" namespace contain the
      // actual bytes of the WebAssembly file. This uses esbuild's
      // built-in "binary" loader instead of manually embedding the
      // binary data inside JavaScript code ourselves.
      build.onLoad({ filter: /.*/, namespace: 'wasm-binary' }, async (args) => ({
        contents: await readFile(args.path),
        loader: 'binary',
      }))
    },
  };

const options = {
  entryPoints: ['./index.ts'],
  bundle: true,
  format: 'esm',
  outfile: 'dist/index.js',
  sourcemap: true,
  plugins: [wasmPlugin],
};

const watch = process.argv.includes('--watch') || process.argv.includes('-w');
if(watch) {
    const ctx = await context(options);
    await ctx.watch();
} else {
  build(options);
}
