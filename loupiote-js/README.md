## Compiling

### wasm-unknown-unknown

```sh
cargo build --target=wasm32-unknown-unknown
```

### Emscripten

```sh
PATH="/Users/davidpeicho/Dev/third_party/emsdk/upstream/emscripten:$PATH" CXXFLAGS="-DRUST_CXX_NO_EXCEPTIONS=ON" cargo build --target=wasm32-unknown-emscripten
```
