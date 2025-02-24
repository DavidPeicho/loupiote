import {build, context} from 'esbuild';
import {isAbsolute, join} from 'path';
import {readFile} from 'fs/promises';

// Inline .wasm imports
const wasmPlugin = {
    name: 'wasm',
    setup(build) {
      build.onResolve({ filter: /\.wasm$/ }, args => {
        if (args.resolveDir === '') return;
        const path = isAbsolute(args.path) ? args.path : join(args.resolveDir, args.path);
        return { path, namespace: 'wasm-binary' };
      }),

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
