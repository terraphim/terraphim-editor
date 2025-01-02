let o;
const T = typeof TextDecoder < "u" ? new TextDecoder("utf-8", { ignoreBOM: !0, fatal: !0 }) : { decode: () => {
  throw Error("TextDecoder not available");
} };
typeof TextDecoder < "u" && T.decode();
let l = null;
function g() {
  return (l === null || l.byteLength === 0) && (l = new Uint8Array(o.memory.buffer)), l;
}
function _(t, e) {
  return t = t >>> 0, T.decode(g().subarray(t, t + e));
}
function f(t) {
  const e = o.__externref_table_alloc();
  return o.__wbindgen_export_2.set(e, t), e;
}
function p(t, e) {
  try {
    return t.apply(this, e);
  } catch (n) {
    const i = f(n);
    o.__wbindgen_exn_store(i);
  }
}
function u(t) {
  return t == null;
}
let m = 0;
const y = typeof TextEncoder < "u" ? new TextEncoder("utf-8") : { encode: () => {
  throw Error("TextEncoder not available");
} }, v = typeof y.encodeInto == "function" ? function(t, e) {
  return y.encodeInto(t, e);
} : function(t, e) {
  const n = y.encode(t);
  return e.set(n), {
    read: t.length,
    written: n.length
  };
};
function h(t, e, n) {
  if (n === void 0) {
    const s = y.encode(t), b = e(s.length, 1) >>> 0;
    return g().subarray(b, b + s.length).set(s), m = s.length, b;
  }
  let i = t.length, r = e(i, 1) >>> 0;
  const a = g();
  let c = 0;
  for (; c < i; c++) {
    const s = t.charCodeAt(c);
    if (s > 127) break;
    a[r + c] = s;
  }
  if (c !== i) {
    c !== 0 && (t = t.slice(c)), r = n(r, i, i = c + t.length * 3, 1) >>> 0;
    const s = g().subarray(r + c, r + i), b = v(t, s);
    c += b.written, r = n(r, i, c, 1) >>> 0;
  }
  return m = c, r;
}
let d = null;
function w() {
  return (d === null || d.buffer.detached === !0 || d.buffer.detached === void 0 && d.buffer !== o.memory.buffer) && (d = new DataView(o.memory.buffer)), d;
}
const E = typeof FinalizationRegistry > "u" ? { register: () => {
}, unregister: () => {
} } : new FinalizationRegistry((t) => {
  o.__wbindgen_export_6.get(t.dtor)(t.a, t.b);
});
function L(t, e, n, i) {
  const r = { a: t, b: e, cnt: 1, dtor: n }, a = (...c) => {
    r.cnt++;
    const s = r.a;
    r.a = 0;
    try {
      return i(s, r.b, ...c);
    } finally {
      --r.cnt === 0 ? (o.__wbindgen_export_6.get(r.dtor)(s, r.b), E.unregister(r)) : r.a = s;
    }
  };
  return a.original = r, E.register(a, r, r), a;
}
function A() {
  o.run();
}
function W(t, e, n) {
  o.closure2_externref_shim(t, e, n);
}
async function M(t, e) {
  if (typeof Response == "function" && t instanceof Response) {
    if (typeof WebAssembly.instantiateStreaming == "function")
      try {
        return await WebAssembly.instantiateStreaming(t, e);
      } catch (i) {
        if (t.headers.get("Content-Type") != "application/wasm")
          console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", i);
        else
          throw i;
      }
    const n = await t.arrayBuffer();
    return await WebAssembly.instantiate(n, e);
  } else {
    const n = await WebAssembly.instantiate(t, e);
    return n instanceof WebAssembly.Instance ? { instance: n, module: t } : n;
  }
}
function S() {
  const t = {};
  return t.wbg = {}, t.wbg.__wbg_addEventListener_b9481c2c2cab6047 = function() {
    return p(function(e, n, i, r) {
      e.addEventListener(_(n, i), r);
    }, arguments);
  }, t.wbg.__wbg_call_b0d8e36992d9900d = function() {
    return p(function(e, n) {
      return e.call(n);
    }, arguments);
  }, t.wbg.__wbg_document_f11bc4f7c03e1745 = function(e) {
    const n = e.document;
    return u(n) ? 0 : f(n);
  }, t.wbg.__wbg_error_7534b8e9a36f1ab4 = function(e, n) {
    let i, r;
    try {
      i = e, r = n, console.error(_(e, n));
    } finally {
      o.__wbindgen_free(i, r, 1);
    }
  }, t.wbg.__wbg_getElementById_dcc9f1f3cfdca0bc = function(e, n, i) {
    const r = e.getElementById(_(n, i));
    return u(r) ? 0 : f(r);
  }, t.wbg.__wbg_instanceof_HtmlDivElement_5853ec72f4da3564 = function(e) {
    let n;
    try {
      n = e instanceof HTMLDivElement;
    } catch {
      n = !1;
    }
    return n;
  }, t.wbg.__wbg_instanceof_HtmlTextAreaElement_88347fc269bfb466 = function(e) {
    let n;
    try {
      n = e instanceof HTMLTextAreaElement;
    } catch {
      n = !1;
    }
    return n;
  }, t.wbg.__wbg_instanceof_Window_d2514c6a7ee7ba60 = function(e) {
    let n;
    try {
      n = e instanceof Window;
    } catch {
      n = !1;
    }
    return n;
  }, t.wbg.__wbg_new_8a6f238a6ece86ea = function() {
    return new Error();
  }, t.wbg.__wbg_newnoargs_fd9e4bf8be2bc16d = function(e, n) {
    return new Function(_(e, n));
  }, t.wbg.__wbg_querySelector_7b4362006fdeda68 = function() {
    return p(function(e, n, i) {
      const r = e.querySelector(_(n, i));
      return u(r) ? 0 : f(r);
    }, arguments);
  }, t.wbg.__wbg_setinnerHTML_2d75307ba8832258 = function(e, n, i) {
    e.innerHTML = _(n, i);
  }, t.wbg.__wbg_stack_0ed75d68575b0f3c = function(e, n) {
    const i = n.stack, r = h(i, o.__wbindgen_malloc, o.__wbindgen_realloc), a = m;
    w().setInt32(e + 4 * 1, a, !0), w().setInt32(e + 4 * 0, r, !0);
  }, t.wbg.__wbg_static_accessor_GLOBAL_0be7472e492ad3e3 = function() {
    const e = typeof global > "u" ? null : global;
    return u(e) ? 0 : f(e);
  }, t.wbg.__wbg_static_accessor_GLOBAL_THIS_1a6eb482d12c9bfb = function() {
    const e = typeof globalThis > "u" ? null : globalThis;
    return u(e) ? 0 : f(e);
  }, t.wbg.__wbg_static_accessor_SELF_1dc398a895c82351 = function() {
    const e = typeof self > "u" ? null : self;
    return u(e) ? 0 : f(e);
  }, t.wbg.__wbg_static_accessor_WINDOW_ae1c80c7eea8d64a = function() {
    const e = typeof window > "u" ? null : window;
    return u(e) ? 0 : f(e);
  }, t.wbg.__wbg_target_a8fe593e7ee79c21 = function(e) {
    const n = e.target;
    return u(n) ? 0 : f(n);
  }, t.wbg.__wbg_value_a8b8b65bc31190d6 = function(e, n) {
    const i = n.value, r = h(i, o.__wbindgen_malloc, o.__wbindgen_realloc), a = m;
    w().setInt32(e + 4 * 1, a, !0), w().setInt32(e + 4 * 0, r, !0);
  }, t.wbg.__wbindgen_cb_drop = function(e) {
    const n = e.original;
    return n.cnt-- == 1 ? (n.a = 0, !0) : !1;
  }, t.wbg.__wbindgen_closure_wrapper30 = function(e, n, i) {
    return L(e, n, 3, W);
  }, t.wbg.__wbindgen_init_externref_table = function() {
    const e = o.__wbindgen_export_2, n = e.grow(4);
    e.set(0, void 0), e.set(n + 0, void 0), e.set(n + 1, null), e.set(n + 2, !0), e.set(n + 3, !1);
  }, t.wbg.__wbindgen_is_undefined = function(e) {
    return e === void 0;
  }, t.wbg.__wbindgen_rethrow = function(e) {
    throw e;
  }, t.wbg.__wbindgen_string_new = function(e, n) {
    return _(e, n);
  }, t.wbg.__wbindgen_throw = function(e, n) {
    throw new Error(_(e, n));
  }, t;
}
function D(t, e) {
  return o = t.exports, x.__wbindgen_wasm_module = e, d = null, l = null, o.__wbindgen_start(), o;
}
async function x(t) {
  if (o !== void 0) return o;
  typeof t < "u" && (Object.getPrototypeOf(t) === Object.prototype ? { module_or_path: t } = t : console.warn("using deprecated parameters for the initialization function; pass a single object instead")), typeof t > "u" && (t = new URL("terraphim_editor_bg.wasm", import.meta.url));
  const e = S();
  (typeof t == "string" || typeof Request == "function" && t instanceof Request || typeof URL == "function" && t instanceof URL) && (t = fetch(t));
  const { instance: n, module: i } = await M(await t, e);
  return D(n, i);
}
class I {
  constructor(e) {
    this.container = e.container, this.config = e.config;
  }
  async initialize() {
    try {
      await x("../wasm/terraphim_editor_bg.wasm"), this.container.innerHTML = `
                <div id="app">
                    <div class="terraphim-editor">
                        <div class="toolbar" id="formatting-toolbar"></div>
                        <div class="editor-area">
                            <textarea class="markdown-input"></textarea>
                            <div class="markdown-preview"></div>
                        </div>
                    </div>
                </div>
            `, A(), this.setupEditor();
    } catch (e) {
      throw console.error("Failed to initialize editor:", e), e;
    }
  }
  setupEditor() {
    console.log("Editor initialized with config:", this.config);
  }
}
typeof window < "u" && (window.TeraphimEditor = I);
export {
  I as TeraphimEditor
};
//# sourceMappingURL=terraphim-editor.es.js.map
