import wasm from '../crates/wasm/pkg/loupiote_bg.wasm';
import _init, { test } from '../crates/wasm/pkg/loupiote.js';

export default function init() {
    return _init(wasm);
};

export class Renderer {
    test() {
        test();
    }
}
