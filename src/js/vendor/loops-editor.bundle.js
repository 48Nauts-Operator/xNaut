var XnautRete = (() => {
  var __create = Object.create;
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __getProtoOf = Object.getPrototypeOf;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __commonJS = (cb, mod) => function __require() {
    try {
      return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
    } catch (e7) {
      throw mod = 0, e7;
    }
  };
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
    // If the importer is in node compatibility mode or this is not an ESM
    // file that has been converted to a CommonJS file using a Babel-
    // compatible transform (i.e. "__esModule" has not been set), then set
    // "default" to the CommonJS "module.exports" for node compatibility.
    isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
    mod
  ));
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

  // node_modules/@babel/runtime/helpers/OverloadYield.js
  var require_OverloadYield = __commonJS({
    "node_modules/@babel/runtime/helpers/OverloadYield.js"(exports, module) {
      function _OverloadYield(e7, d3) {
        this.v = e7, this.k = d3;
      }
      module.exports = _OverloadYield, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regeneratorDefine.js
  var require_regeneratorDefine = __commonJS({
    "node_modules/@babel/runtime/helpers/regeneratorDefine.js"(exports, module) {
      function _regeneratorDefine(e7, r5, n6, t5) {
        var i7 = Object.defineProperty;
        try {
          i7({}, "", {});
        } catch (e8) {
          i7 = 0;
        }
        module.exports = _regeneratorDefine = function regeneratorDefine(e8, r6, n7, t6) {
          function o7(r7, n8) {
            _regeneratorDefine(e8, r7, function(e9) {
              return this._invoke(r7, n8, e9);
            });
          }
          r6 ? i7 ? i7(e8, r6, {
            value: n7,
            enumerable: !t6,
            configurable: !t6,
            writable: !t6
          }) : e8[r6] = n7 : (o7("next", 0), o7("throw", 1), o7("return", 2));
        }, module.exports.__esModule = true, module.exports["default"] = module.exports, _regeneratorDefine(e7, r5, n6, t5);
      }
      module.exports = _regeneratorDefine, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regenerator.js
  var require_regenerator = __commonJS({
    "node_modules/@babel/runtime/helpers/regenerator.js"(exports, module) {
      var regeneratorDefine = require_regeneratorDefine();
      function _regenerator() {
        var e7, t5, r5 = "function" == typeof Symbol ? Symbol : {}, n6 = r5.iterator || "@@iterator", o7 = r5.toStringTag || "@@toStringTag";
        function i7(r6, n7, o8, i8) {
          var c5 = n7 && n7.prototype instanceof Generator ? n7 : Generator, u4 = Object.create(c5.prototype);
          return regeneratorDefine(u4, "_invoke", (function(r7, n8, o9) {
            var i9, c6, u5, f4 = 0, p3 = o9 || [], y3 = false, G = {
              p: 0,
              n: 0,
              v: e7,
              a: d3,
              f: d3.bind(e7, 4),
              d: function d4(t6, r8) {
                return i9 = t6, c6 = 0, u5 = e7, G.n = r8, a3;
              }
            };
            function d3(r8, n9) {
              for (c6 = r8, u5 = n9, t5 = 0; !y3 && f4 && !o10 && t5 < p3.length; t5++) {
                var o10, i10 = p3[t5], d4 = G.p, l3 = i10[2];
                r8 > 3 ? (o10 = l3 === n9) && (u5 = i10[(c6 = i10[4]) ? 5 : (c6 = 3, 3)], i10[4] = i10[5] = e7) : i10[0] <= d4 && ((o10 = r8 < 2 && d4 < i10[1]) ? (c6 = 0, G.v = n9, G.n = i10[1]) : d4 < l3 && (o10 = r8 < 3 || i10[0] > n9 || n9 > l3) && (i10[4] = r8, i10[5] = n9, G.n = l3, c6 = 0));
              }
              if (o10 || r8 > 1) return a3;
              throw y3 = true, n9;
            }
            return function(o10, p4, l3) {
              if (f4 > 1) throw TypeError("Generator is already running");
              for (y3 && 1 === p4 && d3(p4, l3), c6 = p4, u5 = l3; (t5 = c6 < 2 ? e7 : u5) || !y3; ) {
                i9 || (c6 ? c6 < 3 ? (c6 > 1 && (G.n = -1), d3(c6, u5)) : G.n = u5 : G.v = u5);
                try {
                  if (f4 = 2, i9) {
                    if (c6 || (o10 = "next"), t5 = i9[o10]) {
                      if (!(t5 = t5.call(i9, u5))) throw TypeError("iterator result is not an object");
                      if (!t5.done) return t5;
                      u5 = t5.value, c6 < 2 && (c6 = 0);
                    } else 1 === c6 && (t5 = i9["return"]) && t5.call(i9), c6 < 2 && (u5 = TypeError("The iterator does not provide a '" + o10 + "' method"), c6 = 1);
                    i9 = e7;
                  } else if ((t5 = (y3 = G.n < 0) ? u5 : r7.call(n8, G)) !== a3) break;
                } catch (t6) {
                  i9 = e7, c6 = 1, u5 = t6;
                } finally {
                  f4 = 1;
                }
              }
              return {
                value: t5,
                done: y3
              };
            };
          })(r6, o8, i8), true), u4;
        }
        var a3 = {};
        function Generator() {
        }
        function GeneratorFunction() {
        }
        function GeneratorFunctionPrototype() {
        }
        t5 = Object.getPrototypeOf;
        var c4 = [][n6] ? t5(t5([][n6]())) : (regeneratorDefine(t5 = {}, n6, function() {
          return this;
        }), t5), u3 = GeneratorFunctionPrototype.prototype = Generator.prototype = Object.create(c4);
        function f3(e8) {
          return Object.setPrototypeOf ? Object.setPrototypeOf(e8, GeneratorFunctionPrototype) : (e8.__proto__ = GeneratorFunctionPrototype, regeneratorDefine(e8, o7, "GeneratorFunction")), e8.prototype = Object.create(u3), e8;
        }
        return GeneratorFunction.prototype = GeneratorFunctionPrototype, regeneratorDefine(u3, "constructor", GeneratorFunctionPrototype), regeneratorDefine(GeneratorFunctionPrototype, "constructor", GeneratorFunction), GeneratorFunction.displayName = "GeneratorFunction", regeneratorDefine(GeneratorFunctionPrototype, o7, "GeneratorFunction"), regeneratorDefine(u3), regeneratorDefine(u3, o7, "Generator"), regeneratorDefine(u3, n6, function() {
          return this;
        }), regeneratorDefine(u3, "toString", function() {
          return "[object Generator]";
        }), (module.exports = _regenerator = function _regenerator2() {
          return {
            w: i7,
            m: f3
          };
        }, module.exports.__esModule = true, module.exports["default"] = module.exports)();
      }
      module.exports = _regenerator, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regeneratorAsyncIterator.js
  var require_regeneratorAsyncIterator = __commonJS({
    "node_modules/@babel/runtime/helpers/regeneratorAsyncIterator.js"(exports, module) {
      var OverloadYield = require_OverloadYield();
      var regeneratorDefine = require_regeneratorDefine();
      function AsyncIterator(t5, e7) {
        function n6(r6, o7, i7, f3) {
          try {
            var c4 = t5[r6](o7), u3 = c4.value;
            return u3 instanceof OverloadYield ? e7.resolve(u3.v).then(function(t6) {
              n6("next", t6, i7, f3);
            }, function(t6) {
              n6("throw", t6, i7, f3);
            }) : e7.resolve(u3).then(function(t6) {
              c4.value = t6, i7(c4);
            }, function(t6) {
              return n6("throw", t6, i7, f3);
            });
          } catch (t6) {
            f3(t6);
          }
        }
        var r5;
        this.next || (regeneratorDefine(AsyncIterator.prototype), regeneratorDefine(AsyncIterator.prototype, "function" == typeof Symbol && Symbol.asyncIterator || "@asyncIterator", function() {
          return this;
        })), regeneratorDefine(this, "_invoke", function(t6, o7, i7) {
          function f3() {
            return new e7(function(e8, r6) {
              n6(t6, i7, e8, r6);
            });
          }
          return r5 = r5 ? r5.then(f3, f3) : f3();
        }, true);
      }
      module.exports = AsyncIterator, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regeneratorAsyncGen.js
  var require_regeneratorAsyncGen = __commonJS({
    "node_modules/@babel/runtime/helpers/regeneratorAsyncGen.js"(exports, module) {
      var regenerator = require_regenerator();
      var regeneratorAsyncIterator = require_regeneratorAsyncIterator();
      function _regeneratorAsyncGen(r5, e7, t5, o7, n6) {
        return new regeneratorAsyncIterator(regenerator().w(r5, e7, t5, o7), n6 || Promise);
      }
      module.exports = _regeneratorAsyncGen, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regeneratorAsync.js
  var require_regeneratorAsync = __commonJS({
    "node_modules/@babel/runtime/helpers/regeneratorAsync.js"(exports, module) {
      var regeneratorAsyncGen = require_regeneratorAsyncGen();
      function _regeneratorAsync(n6, e7, r5, t5, o7) {
        var a3 = regeneratorAsyncGen(n6, e7, r5, t5, o7);
        return a3.next().then(function(n7) {
          return n7.done ? n7.value : a3.next();
        });
      }
      module.exports = _regeneratorAsync, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regeneratorKeys.js
  var require_regeneratorKeys = __commonJS({
    "node_modules/@babel/runtime/helpers/regeneratorKeys.js"(exports, module) {
      function _regeneratorKeys(e7) {
        var n6 = Object(e7), r5 = [];
        for (var t5 in n6) r5.unshift(t5);
        return function e8() {
          for (; r5.length; ) if ((t5 = r5.pop()) in n6) return e8.value = t5, e8.done = false, e8;
          return e8.done = true, e8;
        };
      }
      module.exports = _regeneratorKeys, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/typeof.js
  var require_typeof = __commonJS({
    "node_modules/@babel/runtime/helpers/typeof.js"(exports, module) {
      function _typeof2(o7) {
        "@babel/helpers - typeof";
        return module.exports = _typeof2 = "function" == typeof Symbol && "symbol" == typeof Symbol.iterator ? function(o8) {
          return typeof o8;
        } : function(o8) {
          return o8 && "function" == typeof Symbol && o8.constructor === Symbol && o8 !== Symbol.prototype ? "symbol" : typeof o8;
        }, module.exports.__esModule = true, module.exports["default"] = module.exports, _typeof2(o7);
      }
      module.exports = _typeof2, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regeneratorValues.js
  var require_regeneratorValues = __commonJS({
    "node_modules/@babel/runtime/helpers/regeneratorValues.js"(exports, module) {
      var _typeof2 = require_typeof()["default"];
      function _regeneratorValues(e7) {
        if (null != e7) {
          var t5 = e7["function" == typeof Symbol && Symbol.iterator || "@@iterator"], r5 = 0;
          if (t5) return t5.call(e7);
          if ("function" == typeof e7.next) return e7;
          if (!isNaN(e7.length)) return {
            next: function next() {
              return e7 && r5 >= e7.length && (e7 = void 0), {
                value: e7 && e7[r5++],
                done: !e7
              };
            }
          };
        }
        throw new TypeError(_typeof2(e7) + " is not iterable");
      }
      module.exports = _regeneratorValues, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/helpers/regeneratorRuntime.js
  var require_regeneratorRuntime = __commonJS({
    "node_modules/@babel/runtime/helpers/regeneratorRuntime.js"(exports, module) {
      var OverloadYield = require_OverloadYield();
      var regenerator = require_regenerator();
      var regeneratorAsync = require_regeneratorAsync();
      var regeneratorAsyncGen = require_regeneratorAsyncGen();
      var regeneratorAsyncIterator = require_regeneratorAsyncIterator();
      var regeneratorKeys = require_regeneratorKeys();
      var regeneratorValues = require_regeneratorValues();
      function _regeneratorRuntime6() {
        "use strict";
        var r5 = regenerator(), e7 = r5.m(_regeneratorRuntime6), t5 = (Object.getPrototypeOf ? Object.getPrototypeOf(e7) : e7.__proto__).constructor;
        function n6(r6) {
          var e8 = "function" == typeof r6 && r6.constructor;
          return !!e8 && (e8 === t5 || "GeneratorFunction" === (e8.displayName || e8.name));
        }
        var o7 = {
          "throw": 1,
          "return": 2,
          "break": 3,
          "continue": 3
        };
        function a3(r6) {
          var e8, t6;
          return function(n7) {
            e8 || (e8 = {
              stop: function stop() {
                return t6(n7.a, 2);
              },
              "catch": function _catch() {
                return n7.v;
              },
              abrupt: function abrupt(r7, e9) {
                return t6(n7.a, o7[r7], e9);
              },
              delegateYield: function delegateYield(r7, o8, a4) {
                return e8.resultName = o8, t6(n7.d, regeneratorValues(r7), a4);
              },
              finish: function finish(r7) {
                return t6(n7.f, r7);
              }
            }, t6 = function t7(r7, _t, o8) {
              n7.p = e8.prev, n7.n = e8.next;
              try {
                return r7(_t, o8);
              } finally {
                e8.next = n7.n;
              }
            }), e8.resultName && (e8[e8.resultName] = n7.v, e8.resultName = void 0), e8.sent = n7.v, e8.next = n7.n;
            try {
              return r6.call(this, e8);
            } finally {
              n7.p = e8.prev, n7.n = e8.next;
            }
          };
        }
        return (module.exports = _regeneratorRuntime6 = function _regeneratorRuntime7() {
          return {
            wrap: function wrap(e8, t6, n7, o8) {
              return r5.w(a3(e8), t6, n7, o8 && o8.reverse());
            },
            isGeneratorFunction: n6,
            mark: r5.m,
            awrap: function awrap(r6, e8) {
              return new OverloadYield(r6, e8);
            },
            AsyncIterator: regeneratorAsyncIterator,
            async: function async(r6, e8, t6, o8, u3) {
              return (n6(e8) ? regeneratorAsyncGen : regeneratorAsync)(a3(r6), e8, t6, o8, u3);
            },
            keys: regeneratorKeys,
            values: regeneratorValues
          };
        }, module.exports.__esModule = true, module.exports["default"] = module.exports)();
      }
      module.exports = _regeneratorRuntime6, module.exports.__esModule = true, module.exports["default"] = module.exports;
    }
  });

  // node_modules/@babel/runtime/regenerator/index.js
  var require_regenerator2 = __commonJS({
    "node_modules/@babel/runtime/regenerator/index.js"(exports, module) {
      var runtime = require_regeneratorRuntime()();
      module.exports = runtime;
      try {
        regeneratorRuntime = runtime;
      } catch (accidentalStrictMode) {
        if (typeof globalThis === "object") {
          globalThis.regeneratorRuntime = runtime;
        } else {
          Function("r", "regeneratorRuntime = r")(runtime);
        }
      }
    }
  });

  // frontend/loops-editor-entry.js
  var loops_editor_entry_exports = {};
  __export(loops_editor_entry_exports, {
    createEditor: () => createEditor
  });

  // node_modules/@lit/reactive-element/css-tag.js
  var t = globalThis;
  var e = t.ShadowRoot && (void 0 === t.ShadyCSS || t.ShadyCSS.nativeShadow) && "adoptedStyleSheets" in Document.prototype && "replace" in CSSStyleSheet.prototype;
  var s = /* @__PURE__ */ Symbol();
  var o = /* @__PURE__ */ new WeakMap();
  var n = class {
    constructor(t5, e7, o7) {
      if (this._$cssResult$ = true, o7 !== s) throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");
      this.cssText = t5, this.t = e7;
    }
    get styleSheet() {
      let t5 = this.o;
      const s4 = this.t;
      if (e && void 0 === t5) {
        const e7 = void 0 !== s4 && 1 === s4.length;
        e7 && (t5 = o.get(s4)), void 0 === t5 && ((this.o = t5 = new CSSStyleSheet()).replaceSync(this.cssText), e7 && o.set(s4, t5));
      }
      return t5;
    }
    toString() {
      return this.cssText;
    }
  };
  var r = (t5) => new n("string" == typeof t5 ? t5 : t5 + "", void 0, s);
  var i = (t5, ...e7) => {
    const o7 = 1 === t5.length ? t5[0] : e7.reduce((e8, s4, o8) => e8 + ((t6) => {
      if (true === t6._$cssResult$) return t6.cssText;
      if ("number" == typeof t6) return t6;
      throw Error("Value passed to 'css' function must be a 'css' function result: " + t6 + ". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.");
    })(s4) + t5[o8 + 1], t5[0]);
    return new n(o7, t5, s);
  };
  var S = (s4, o7) => {
    if (e) s4.adoptedStyleSheets = o7.map((t5) => t5 instanceof CSSStyleSheet ? t5 : t5.styleSheet);
    else for (const e7 of o7) {
      const o8 = document.createElement("style"), n6 = t.litNonce;
      void 0 !== n6 && o8.setAttribute("nonce", n6), o8.textContent = e7.cssText, s4.appendChild(o8);
    }
  };
  var c = e ? (t5) => t5 : (t5) => t5 instanceof CSSStyleSheet ? ((t6) => {
    let e7 = "";
    for (const s4 of t6.cssRules) e7 += s4.cssText;
    return r(e7);
  })(t5) : t5;

  // node_modules/@lit/reactive-element/reactive-element.js
  var { is: i2, defineProperty: e2, getOwnPropertyDescriptor: h, getOwnPropertyNames: r2, getOwnPropertySymbols: o2, getPrototypeOf: n2 } = Object;
  var a = globalThis;
  var c2 = a.trustedTypes;
  var l = c2 ? c2.emptyScript : "";
  var p = a.reactiveElementPolyfillSupport;
  var d = (t5, s4) => t5;
  var u = { toAttribute(t5, s4) {
    switch (s4) {
      case Boolean:
        t5 = t5 ? l : null;
        break;
      case Object:
      case Array:
        t5 = null == t5 ? t5 : JSON.stringify(t5);
    }
    return t5;
  }, fromAttribute(t5, s4) {
    let i7 = t5;
    switch (s4) {
      case Boolean:
        i7 = null !== t5;
        break;
      case Number:
        i7 = null === t5 ? null : Number(t5);
        break;
      case Object:
      case Array:
        try {
          i7 = JSON.parse(t5);
        } catch (t6) {
          i7 = null;
        }
    }
    return i7;
  } };
  var f = (t5, s4) => !i2(t5, s4);
  var b = { attribute: true, type: String, converter: u, reflect: false, useDefault: false, hasChanged: f };
  Symbol.metadata ??= /* @__PURE__ */ Symbol("metadata"), a.litPropertyMetadata ??= /* @__PURE__ */ new WeakMap();
  var y = class extends HTMLElement {
    static addInitializer(t5) {
      this._$Ei(), (this.l ??= []).push(t5);
    }
    static get observedAttributes() {
      return this.finalize(), this._$Eh && [...this._$Eh.keys()];
    }
    static createProperty(t5, s4 = b) {
      if (s4.state && (s4.attribute = false), this._$Ei(), this.prototype.hasOwnProperty(t5) && ((s4 = Object.create(s4)).wrapped = true), this.elementProperties.set(t5, s4), !s4.noAccessor) {
        const i7 = /* @__PURE__ */ Symbol(), h3 = this.getPropertyDescriptor(t5, i7, s4);
        void 0 !== h3 && e2(this.prototype, t5, h3);
      }
    }
    static getPropertyDescriptor(t5, s4, i7) {
      const { get: e7, set: r5 } = h(this.prototype, t5) ?? { get() {
        return this[s4];
      }, set(t6) {
        this[s4] = t6;
      } };
      return { get: e7, set(s5) {
        const h3 = e7?.call(this);
        r5?.call(this, s5), this.requestUpdate(t5, h3, i7);
      }, configurable: true, enumerable: true };
    }
    static getPropertyOptions(t5) {
      return this.elementProperties.get(t5) ?? b;
    }
    static _$Ei() {
      if (this.hasOwnProperty(d("elementProperties"))) return;
      const t5 = n2(this);
      t5.finalize(), void 0 !== t5.l && (this.l = [...t5.l]), this.elementProperties = new Map(t5.elementProperties);
    }
    static finalize() {
      if (this.hasOwnProperty(d("finalized"))) return;
      if (this.finalized = true, this._$Ei(), this.hasOwnProperty(d("properties"))) {
        const t6 = this.properties, s4 = [...r2(t6), ...o2(t6)];
        for (const i7 of s4) this.createProperty(i7, t6[i7]);
      }
      const t5 = this[Symbol.metadata];
      if (null !== t5) {
        const s4 = litPropertyMetadata.get(t5);
        if (void 0 !== s4) for (const [t6, i7] of s4) this.elementProperties.set(t6, i7);
      }
      this._$Eh = /* @__PURE__ */ new Map();
      for (const [t6, s4] of this.elementProperties) {
        const i7 = this._$Eu(t6, s4);
        void 0 !== i7 && this._$Eh.set(i7, t6);
      }
      this.elementStyles = this.finalizeStyles(this.styles);
    }
    static finalizeStyles(s4) {
      const i7 = [];
      if (Array.isArray(s4)) {
        const e7 = new Set(s4.flat(1 / 0).reverse());
        for (const s5 of e7) i7.unshift(c(s5));
      } else void 0 !== s4 && i7.push(c(s4));
      return i7;
    }
    static _$Eu(t5, s4) {
      const i7 = s4.attribute;
      return false === i7 ? void 0 : "string" == typeof i7 ? i7 : "string" == typeof t5 ? t5.toLowerCase() : void 0;
    }
    constructor() {
      super(), this._$Ep = void 0, this.isUpdatePending = false, this.hasUpdated = false, this._$Em = null, this._$Ev();
    }
    _$Ev() {
      this._$ES = new Promise((t5) => this.enableUpdating = t5), this._$AL = /* @__PURE__ */ new Map(), this._$E_(), this.requestUpdate(), this.constructor.l?.forEach((t5) => t5(this));
    }
    addController(t5) {
      (this._$EO ??= /* @__PURE__ */ new Set()).add(t5), void 0 !== this.renderRoot && this.isConnected && t5.hostConnected?.();
    }
    removeController(t5) {
      this._$EO?.delete(t5);
    }
    _$E_() {
      const t5 = /* @__PURE__ */ new Map(), s4 = this.constructor.elementProperties;
      for (const i7 of s4.keys()) this.hasOwnProperty(i7) && (t5.set(i7, this[i7]), delete this[i7]);
      t5.size > 0 && (this._$Ep = t5);
    }
    createRenderRoot() {
      const t5 = this.shadowRoot ?? this.attachShadow(this.constructor.shadowRootOptions);
      return S(t5, this.constructor.elementStyles), t5;
    }
    connectedCallback() {
      this.renderRoot ??= this.createRenderRoot(), this.enableUpdating(true), this._$EO?.forEach((t5) => t5.hostConnected?.());
    }
    enableUpdating(t5) {
    }
    disconnectedCallback() {
      this._$EO?.forEach((t5) => t5.hostDisconnected?.());
    }
    attributeChangedCallback(t5, s4, i7) {
      this._$AK(t5, i7);
    }
    _$ET(t5, s4) {
      const i7 = this.constructor.elementProperties.get(t5), e7 = this.constructor._$Eu(t5, i7);
      if (void 0 !== e7 && true === i7.reflect) {
        const h3 = (void 0 !== i7.converter?.toAttribute ? i7.converter : u).toAttribute(s4, i7.type);
        this._$Em = t5, null == h3 ? this.removeAttribute(e7) : this.setAttribute(e7, h3), this._$Em = null;
      }
    }
    _$AK(t5, s4) {
      const i7 = this.constructor, e7 = i7._$Eh.get(t5);
      if (void 0 !== e7 && this._$Em !== e7) {
        const t6 = i7.getPropertyOptions(e7), h3 = "function" == typeof t6.converter ? { fromAttribute: t6.converter } : void 0 !== t6.converter?.fromAttribute ? t6.converter : u;
        this._$Em = e7;
        const r5 = h3.fromAttribute(s4, t6.type);
        this[e7] = r5 ?? this._$Ej?.get(e7) ?? r5, this._$Em = null;
      }
    }
    requestUpdate(t5, s4, i7, e7 = false, h3) {
      if (void 0 !== t5) {
        const r5 = this.constructor;
        if (false === e7 && (h3 = this[t5]), i7 ??= r5.getPropertyOptions(t5), !((i7.hasChanged ?? f)(h3, s4) || i7.useDefault && i7.reflect && h3 === this._$Ej?.get(t5) && !this.hasAttribute(r5._$Eu(t5, i7)))) return;
        this.C(t5, s4, i7);
      }
      false === this.isUpdatePending && (this._$ES = this._$EP());
    }
    C(t5, s4, { useDefault: i7, reflect: e7, wrapped: h3 }, r5) {
      i7 && !(this._$Ej ??= /* @__PURE__ */ new Map()).has(t5) && (this._$Ej.set(t5, r5 ?? s4 ?? this[t5]), true !== h3 || void 0 !== r5) || (this._$AL.has(t5) || (this.hasUpdated || i7 || (s4 = void 0), this._$AL.set(t5, s4)), true === e7 && this._$Em !== t5 && (this._$Eq ??= /* @__PURE__ */ new Set()).add(t5));
    }
    async _$EP() {
      this.isUpdatePending = true;
      try {
        await this._$ES;
      } catch (t6) {
        Promise.reject(t6);
      }
      const t5 = this.scheduleUpdate();
      return null != t5 && await t5, !this.isUpdatePending;
    }
    scheduleUpdate() {
      return this.performUpdate();
    }
    performUpdate() {
      if (!this.isUpdatePending) return;
      if (!this.hasUpdated) {
        if (this.renderRoot ??= this.createRenderRoot(), this._$Ep) {
          for (const [t7, s5] of this._$Ep) this[t7] = s5;
          this._$Ep = void 0;
        }
        const t6 = this.constructor.elementProperties;
        if (t6.size > 0) for (const [s5, i7] of t6) {
          const { wrapped: t7 } = i7, e7 = this[s5];
          true !== t7 || this._$AL.has(s5) || void 0 === e7 || this.C(s5, void 0, i7, e7);
        }
      }
      let t5 = false;
      const s4 = this._$AL;
      try {
        t5 = this.shouldUpdate(s4), t5 ? (this.willUpdate(s4), this._$EO?.forEach((t6) => t6.hostUpdate?.()), this.update(s4)) : this._$EM();
      } catch (s5) {
        throw t5 = false, this._$EM(), s5;
      }
      t5 && this._$AE(s4);
    }
    willUpdate(t5) {
    }
    _$AE(t5) {
      this._$EO?.forEach((t6) => t6.hostUpdated?.()), this.hasUpdated || (this.hasUpdated = true, this.firstUpdated(t5)), this.updated(t5);
    }
    _$EM() {
      this._$AL = /* @__PURE__ */ new Map(), this.isUpdatePending = false;
    }
    get updateComplete() {
      return this.getUpdateComplete();
    }
    getUpdateComplete() {
      return this._$ES;
    }
    shouldUpdate(t5) {
      return true;
    }
    update(t5) {
      this._$Eq &&= this._$Eq.forEach((t6) => this._$ET(t6, this[t6])), this._$EM();
    }
    updated(t5) {
    }
    firstUpdated(t5) {
    }
  };
  y.elementStyles = [], y.shadowRootOptions = { mode: "open" }, y[d("elementProperties")] = /* @__PURE__ */ new Map(), y[d("finalized")] = /* @__PURE__ */ new Map(), p?.({ ReactiveElement: y }), (a.reactiveElementVersions ??= []).push("2.1.2");

  // node_modules/lit-html/lit-html.js
  var t2 = globalThis;
  var i3 = (t5) => t5;
  var s2 = t2.trustedTypes;
  var e3 = s2 ? s2.createPolicy("lit-html", { createHTML: (t5) => t5 }) : void 0;
  var h2 = "$lit$";
  var o3 = `lit$${Math.random().toFixed(9).slice(2)}$`;
  var n3 = "?" + o3;
  var r3 = `<${n3}>`;
  var l2 = document;
  var c3 = () => l2.createComment("");
  var a2 = (t5) => null === t5 || "object" != typeof t5 && "function" != typeof t5;
  var u2 = Array.isArray;
  var d2 = (t5) => u2(t5) || "function" == typeof t5?.[Symbol.iterator];
  var f2 = "[ 	\n\f\r]";
  var v = /<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g;
  var _ = /-->/g;
  var m = />/g;
  var p2 = RegExp(`>|${f2}(?:([^\\s"'>=/]+)(${f2}*=${f2}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`, "g");
  var g = /'/g;
  var $ = /"/g;
  var y2 = /^(?:script|style|textarea|title)$/i;
  var x = (t5) => (i7, ...s4) => ({ _$litType$: t5, strings: i7, values: s4 });
  var b2 = x(1);
  var w = x(2);
  var T = x(3);
  var E = /* @__PURE__ */ Symbol.for("lit-noChange");
  var A = /* @__PURE__ */ Symbol.for("lit-nothing");
  var C = /* @__PURE__ */ new WeakMap();
  var P = l2.createTreeWalker(l2, 129);
  function V(t5, i7) {
    if (!u2(t5) || !t5.hasOwnProperty("raw")) throw Error("invalid template strings array");
    return void 0 !== e3 ? e3.createHTML(i7) : i7;
  }
  var N = (t5, i7) => {
    const s4 = t5.length - 1, e7 = [];
    let n6, l3 = 2 === i7 ? "<svg>" : 3 === i7 ? "<math>" : "", c4 = v;
    for (let i8 = 0; i8 < s4; i8++) {
      const s5 = t5[i8];
      let a3, u3, d3 = -1, f3 = 0;
      for (; f3 < s5.length && (c4.lastIndex = f3, u3 = c4.exec(s5), null !== u3); ) f3 = c4.lastIndex, c4 === v ? "!--" === u3[1] ? c4 = _ : void 0 !== u3[1] ? c4 = m : void 0 !== u3[2] ? (y2.test(u3[2]) && (n6 = RegExp("</" + u3[2], "g")), c4 = p2) : void 0 !== u3[3] && (c4 = p2) : c4 === p2 ? ">" === u3[0] ? (c4 = n6 ?? v, d3 = -1) : void 0 === u3[1] ? d3 = -2 : (d3 = c4.lastIndex - u3[2].length, a3 = u3[1], c4 = void 0 === u3[3] ? p2 : '"' === u3[3] ? $ : g) : c4 === $ || c4 === g ? c4 = p2 : c4 === _ || c4 === m ? c4 = v : (c4 = p2, n6 = void 0);
      const x2 = c4 === p2 && t5[i8 + 1].startsWith("/>") ? " " : "";
      l3 += c4 === v ? s5 + r3 : d3 >= 0 ? (e7.push(a3), s5.slice(0, d3) + h2 + s5.slice(d3) + o3 + x2) : s5 + o3 + (-2 === d3 ? i8 : x2);
    }
    return [V(t5, l3 + (t5[s4] || "<?>") + (2 === i7 ? "</svg>" : 3 === i7 ? "</math>" : "")), e7];
  };
  var S2 = class _S {
    constructor({ strings: t5, _$litType$: i7 }, e7) {
      let r5;
      this.parts = [];
      let l3 = 0, a3 = 0;
      const u3 = t5.length - 1, d3 = this.parts, [f3, v2] = N(t5, i7);
      if (this.el = _S.createElement(f3, e7), P.currentNode = this.el.content, 2 === i7 || 3 === i7) {
        const t6 = this.el.content.firstChild;
        t6.replaceWith(...t6.childNodes);
      }
      for (; null !== (r5 = P.nextNode()) && d3.length < u3; ) {
        if (1 === r5.nodeType) {
          if (r5.hasAttributes()) for (const t6 of r5.getAttributeNames()) if (t6.endsWith(h2)) {
            const i8 = v2[a3++], s4 = r5.getAttribute(t6).split(o3), e8 = /([.?@])?(.*)/.exec(i8);
            d3.push({ type: 1, index: l3, name: e8[2], strings: s4, ctor: "." === e8[1] ? I : "?" === e8[1] ? L : "@" === e8[1] ? z : H }), r5.removeAttribute(t6);
          } else t6.startsWith(o3) && (d3.push({ type: 6, index: l3 }), r5.removeAttribute(t6));
          if (y2.test(r5.tagName)) {
            const t6 = r5.textContent.split(o3), i8 = t6.length - 1;
            if (i8 > 0) {
              r5.textContent = s2 ? s2.emptyScript : "";
              for (let s4 = 0; s4 < i8; s4++) r5.append(t6[s4], c3()), P.nextNode(), d3.push({ type: 2, index: ++l3 });
              r5.append(t6[i8], c3());
            }
          }
        } else if (8 === r5.nodeType) if (r5.data === n3) d3.push({ type: 2, index: l3 });
        else {
          let t6 = -1;
          for (; -1 !== (t6 = r5.data.indexOf(o3, t6 + 1)); ) d3.push({ type: 7, index: l3 }), t6 += o3.length - 1;
        }
        l3++;
      }
    }
    static createElement(t5, i7) {
      const s4 = l2.createElement("template");
      return s4.innerHTML = t5, s4;
    }
  };
  function M(t5, i7, s4 = t5, e7) {
    if (i7 === E) return i7;
    let h3 = void 0 !== e7 ? s4._$Co?.[e7] : s4._$Cl;
    const o7 = a2(i7) ? void 0 : i7._$litDirective$;
    return h3?.constructor !== o7 && (h3?._$AO?.(false), void 0 === o7 ? h3 = void 0 : (h3 = new o7(t5), h3._$AT(t5, s4, e7)), void 0 !== e7 ? (s4._$Co ??= [])[e7] = h3 : s4._$Cl = h3), void 0 !== h3 && (i7 = M(t5, h3._$AS(t5, i7.values), h3, e7)), i7;
  }
  var R = class {
    constructor(t5, i7) {
      this._$AV = [], this._$AN = void 0, this._$AD = t5, this._$AM = i7;
    }
    get parentNode() {
      return this._$AM.parentNode;
    }
    get _$AU() {
      return this._$AM._$AU;
    }
    u(t5) {
      const { el: { content: i7 }, parts: s4 } = this._$AD, e7 = (t5?.creationScope ?? l2).importNode(i7, true);
      P.currentNode = e7;
      let h3 = P.nextNode(), o7 = 0, n6 = 0, r5 = s4[0];
      for (; void 0 !== r5; ) {
        if (o7 === r5.index) {
          let i8;
          2 === r5.type ? i8 = new k(h3, h3.nextSibling, this, t5) : 1 === r5.type ? i8 = new r5.ctor(h3, r5.name, r5.strings, this, t5) : 6 === r5.type && (i8 = new Z(h3, this, t5)), this._$AV.push(i8), r5 = s4[++n6];
        }
        o7 !== r5?.index && (h3 = P.nextNode(), o7++);
      }
      return P.currentNode = l2, e7;
    }
    p(t5) {
      let i7 = 0;
      for (const s4 of this._$AV) void 0 !== s4 && (void 0 !== s4.strings ? (s4._$AI(t5, s4, i7), i7 += s4.strings.length - 2) : s4._$AI(t5[i7])), i7++;
    }
  };
  var k = class _k {
    get _$AU() {
      return this._$AM?._$AU ?? this._$Cv;
    }
    constructor(t5, i7, s4, e7) {
      this.type = 2, this._$AH = A, this._$AN = void 0, this._$AA = t5, this._$AB = i7, this._$AM = s4, this.options = e7, this._$Cv = e7?.isConnected ?? true;
    }
    get parentNode() {
      let t5 = this._$AA.parentNode;
      const i7 = this._$AM;
      return void 0 !== i7 && 11 === t5?.nodeType && (t5 = i7.parentNode), t5;
    }
    get startNode() {
      return this._$AA;
    }
    get endNode() {
      return this._$AB;
    }
    _$AI(t5, i7 = this) {
      t5 = M(this, t5, i7), a2(t5) ? t5 === A || null == t5 || "" === t5 ? (this._$AH !== A && this._$AR(), this._$AH = A) : t5 !== this._$AH && t5 !== E && this._(t5) : void 0 !== t5._$litType$ ? this.$(t5) : void 0 !== t5.nodeType ? this.T(t5) : d2(t5) ? this.k(t5) : this._(t5);
    }
    O(t5) {
      return this._$AA.parentNode.insertBefore(t5, this._$AB);
    }
    T(t5) {
      this._$AH !== t5 && (this._$AR(), this._$AH = this.O(t5));
    }
    _(t5) {
      this._$AH !== A && a2(this._$AH) ? this._$AA.nextSibling.data = t5 : this.T(l2.createTextNode(t5)), this._$AH = t5;
    }
    $(t5) {
      const { values: i7, _$litType$: s4 } = t5, e7 = "number" == typeof s4 ? this._$AC(t5) : (void 0 === s4.el && (s4.el = S2.createElement(V(s4.h, s4.h[0]), this.options)), s4);
      if (this._$AH?._$AD === e7) this._$AH.p(i7);
      else {
        const t6 = new R(e7, this), s5 = t6.u(this.options);
        t6.p(i7), this.T(s5), this._$AH = t6;
      }
    }
    _$AC(t5) {
      let i7 = C.get(t5.strings);
      return void 0 === i7 && C.set(t5.strings, i7 = new S2(t5)), i7;
    }
    k(t5) {
      u2(this._$AH) || (this._$AH = [], this._$AR());
      const i7 = this._$AH;
      let s4, e7 = 0;
      for (const h3 of t5) e7 === i7.length ? i7.push(s4 = new _k(this.O(c3()), this.O(c3()), this, this.options)) : s4 = i7[e7], s4._$AI(h3), e7++;
      e7 < i7.length && (this._$AR(s4 && s4._$AB.nextSibling, e7), i7.length = e7);
    }
    _$AR(t5 = this._$AA.nextSibling, s4) {
      for (this._$AP?.(false, true, s4); t5 !== this._$AB; ) {
        const s5 = i3(t5).nextSibling;
        i3(t5).remove(), t5 = s5;
      }
    }
    setConnected(t5) {
      void 0 === this._$AM && (this._$Cv = t5, this._$AP?.(t5));
    }
  };
  var H = class {
    get tagName() {
      return this.element.tagName;
    }
    get _$AU() {
      return this._$AM._$AU;
    }
    constructor(t5, i7, s4, e7, h3) {
      this.type = 1, this._$AH = A, this._$AN = void 0, this.element = t5, this.name = i7, this._$AM = e7, this.options = h3, s4.length > 2 || "" !== s4[0] || "" !== s4[1] ? (this._$AH = Array(s4.length - 1).fill(new String()), this.strings = s4) : this._$AH = A;
    }
    _$AI(t5, i7 = this, s4, e7) {
      const h3 = this.strings;
      let o7 = false;
      if (void 0 === h3) t5 = M(this, t5, i7, 0), o7 = !a2(t5) || t5 !== this._$AH && t5 !== E, o7 && (this._$AH = t5);
      else {
        const e8 = t5;
        let n6, r5;
        for (t5 = h3[0], n6 = 0; n6 < h3.length - 1; n6++) r5 = M(this, e8[s4 + n6], i7, n6), r5 === E && (r5 = this._$AH[n6]), o7 ||= !a2(r5) || r5 !== this._$AH[n6], r5 === A ? t5 = A : t5 !== A && (t5 += (r5 ?? "") + h3[n6 + 1]), this._$AH[n6] = r5;
      }
      o7 && !e7 && this.j(t5);
    }
    j(t5) {
      t5 === A ? this.element.removeAttribute(this.name) : this.element.setAttribute(this.name, t5 ?? "");
    }
  };
  var I = class extends H {
    constructor() {
      super(...arguments), this.type = 3;
    }
    j(t5) {
      this.element[this.name] = t5 === A ? void 0 : t5;
    }
  };
  var L = class extends H {
    constructor() {
      super(...arguments), this.type = 4;
    }
    j(t5) {
      this.element.toggleAttribute(this.name, !!t5 && t5 !== A);
    }
  };
  var z = class extends H {
    constructor(t5, i7, s4, e7, h3) {
      super(t5, i7, s4, e7, h3), this.type = 5;
    }
    _$AI(t5, i7 = this) {
      if ((t5 = M(this, t5, i7, 0) ?? A) === E) return;
      const s4 = this._$AH, e7 = t5 === A && s4 !== A || t5.capture !== s4.capture || t5.once !== s4.once || t5.passive !== s4.passive, h3 = t5 !== A && (s4 === A || e7);
      e7 && this.element.removeEventListener(this.name, this, s4), h3 && this.element.addEventListener(this.name, this, t5), this._$AH = t5;
    }
    handleEvent(t5) {
      "function" == typeof this._$AH ? this._$AH.call(this.options?.host ?? this.element, t5) : this._$AH.handleEvent(t5);
    }
  };
  var Z = class {
    constructor(t5, i7, s4) {
      this.element = t5, this.type = 6, this._$AN = void 0, this._$AM = i7, this.options = s4;
    }
    get _$AU() {
      return this._$AM._$AU;
    }
    _$AI(t5) {
      M(this, t5);
    }
  };
  var B = t2.litHtmlPolyfillSupport;
  B?.(S2, k), (t2.litHtmlVersions ??= []).push("3.3.3");
  var D = (t5, i7, s4) => {
    const e7 = s4?.renderBefore ?? i7;
    let h3 = e7._$litPart$;
    if (void 0 === h3) {
      const t6 = s4?.renderBefore ?? null;
      e7._$litPart$ = h3 = new k(i7.insertBefore(c3(), t6), t6, void 0, s4 ?? {});
    }
    return h3._$AI(t5), h3;
  };

  // node_modules/lit-element/lit-element.js
  var s3 = globalThis;
  var i4 = class extends y {
    constructor() {
      super(...arguments), this.renderOptions = { host: this }, this._$Do = void 0;
    }
    createRenderRoot() {
      const t5 = super.createRenderRoot();
      return this.renderOptions.renderBefore ??= t5.firstChild, t5;
    }
    update(t5) {
      const r5 = this.render();
      this.hasUpdated || (this.renderOptions.isConnected = this.isConnected), super.update(t5), this._$Do = D(r5, this.renderRoot, this.renderOptions);
    }
    connectedCallback() {
      super.connectedCallback(), this._$Do?.setConnected(true);
    }
    disconnectedCallback() {
      super.disconnectedCallback(), this._$Do?.setConnected(false);
    }
    render() {
      return E;
    }
  };
  i4._$litElement$ = true, i4["finalized"] = true, s3.litElementHydrateSupport?.({ LitElement: i4 });
  var o4 = s3.litElementPolyfillSupport;
  o4?.({ LitElement: i4 });
  (s3.litElementVersions ??= []).push("4.2.2");

  // node_modules/@babel/runtime/helpers/esm/asyncToGenerator.js
  function asyncGeneratorStep(n6, t5, e7, r5, o7, a3, c4) {
    try {
      var i7 = n6[a3](c4), u3 = i7.value;
    } catch (n7) {
      return void e7(n7);
    }
    i7.done ? t5(u3) : Promise.resolve(u3).then(r5, o7);
  }
  function _asyncToGenerator(n6) {
    return function() {
      var t5 = this, e7 = arguments;
      return new Promise(function(r5, o7) {
        var a3 = n6.apply(t5, e7);
        function _next(n7) {
          asyncGeneratorStep(a3, r5, o7, _next, _throw, "next", n7);
        }
        function _throw(n7) {
          asyncGeneratorStep(a3, r5, o7, _next, _throw, "throw", n7);
        }
        _next(void 0);
      });
    };
  }

  // node_modules/@babel/runtime/helpers/esm/classCallCheck.js
  function _classCallCheck(a3, n6) {
    if (!(a3 instanceof n6)) throw new TypeError("Cannot call a class as a function");
  }

  // node_modules/@babel/runtime/helpers/esm/typeof.js
  function _typeof(o7) {
    "@babel/helpers - typeof";
    return _typeof = "function" == typeof Symbol && "symbol" == typeof Symbol.iterator ? function(o8) {
      return typeof o8;
    } : function(o8) {
      return o8 && "function" == typeof Symbol && o8.constructor === Symbol && o8 !== Symbol.prototype ? "symbol" : typeof o8;
    }, _typeof(o7);
  }

  // node_modules/@babel/runtime/helpers/esm/toPrimitive.js
  function toPrimitive(t5, r5) {
    if ("object" != _typeof(t5) || !t5) return t5;
    var e7 = t5[Symbol.toPrimitive];
    if (void 0 !== e7) {
      var i7 = e7.call(t5, r5 || "default");
      if ("object" != _typeof(i7)) return i7;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return ("string" === r5 ? String : Number)(t5);
  }

  // node_modules/@babel/runtime/helpers/esm/toPropertyKey.js
  function toPropertyKey(t5) {
    var i7 = toPrimitive(t5, "string");
    return "symbol" == _typeof(i7) ? i7 : i7 + "";
  }

  // node_modules/@babel/runtime/helpers/esm/createClass.js
  function _defineProperties(e7, r5) {
    for (var t5 = 0; t5 < r5.length; t5++) {
      var o7 = r5[t5];
      o7.enumerable = o7.enumerable || false, o7.configurable = true, "value" in o7 && (o7.writable = true), Object.defineProperty(e7, toPropertyKey(o7.key), o7);
    }
  }
  function _createClass(e7, r5, t5) {
    return r5 && _defineProperties(e7.prototype, r5), t5 && _defineProperties(e7, t5), Object.defineProperty(e7, "prototype", {
      writable: false
    }), e7;
  }

  // node_modules/@babel/runtime/helpers/esm/assertThisInitialized.js
  function _assertThisInitialized(e7) {
    if (void 0 === e7) throw new ReferenceError("this hasn't been initialised - super() hasn't been called");
    return e7;
  }

  // node_modules/@babel/runtime/helpers/esm/possibleConstructorReturn.js
  function _possibleConstructorReturn(t5, e7) {
    if (e7 && ("object" == _typeof(e7) || "function" == typeof e7)) return e7;
    if (void 0 !== e7) throw new TypeError("Derived constructors may only return object or undefined");
    return _assertThisInitialized(t5);
  }

  // node_modules/@babel/runtime/helpers/esm/getPrototypeOf.js
  function _getPrototypeOf(t5) {
    return _getPrototypeOf = Object.setPrototypeOf ? Object.getPrototypeOf.bind() : function(t6) {
      return t6.__proto__ || Object.getPrototypeOf(t6);
    }, _getPrototypeOf(t5);
  }

  // node_modules/@babel/runtime/helpers/esm/setPrototypeOf.js
  function _setPrototypeOf(t5, e7) {
    return _setPrototypeOf = Object.setPrototypeOf ? Object.setPrototypeOf.bind() : function(t6, e8) {
      return t6.__proto__ = e8, t6;
    }, _setPrototypeOf(t5, e7);
  }

  // node_modules/@babel/runtime/helpers/esm/inherits.js
  function _inherits(t5, e7) {
    if ("function" != typeof e7 && null !== e7) throw new TypeError("Super expression must either be null or a function");
    t5.prototype = Object.create(e7 && e7.prototype, {
      constructor: {
        value: t5,
        writable: true,
        configurable: true
      }
    }), Object.defineProperty(t5, "prototype", {
      writable: false
    }), e7 && _setPrototypeOf(t5, e7);
  }

  // node_modules/@babel/runtime/helpers/esm/defineProperty.js
  function _defineProperty(e7, r5, t5) {
    return (r5 = toPropertyKey(r5)) in e7 ? Object.defineProperty(e7, r5, {
      value: t5,
      enumerable: true,
      configurable: true,
      writable: true
    }) : e7[r5] = t5, e7;
  }

  // node_modules/rete/rete.esm.js
  var import_regenerator = __toESM(require_regenerator2());
  function _createForOfIteratorHelper$1(r5, e7) {
    var t5 = "undefined" != typeof Symbol && r5[Symbol.iterator] || r5["@@iterator"];
    if (!t5) {
      if (Array.isArray(r5) || (t5 = _unsupportedIterableToArray$1(r5)) || e7 && r5 && "number" == typeof r5.length) {
        t5 && (r5 = t5);
        var _n = 0, F = function F2() {
        };
        return { s: F, n: function n6() {
          return _n >= r5.length ? { done: true } : { done: false, value: r5[_n++] };
        }, e: function e8(r6) {
          throw r6;
        }, f: F };
      }
      throw new TypeError("Invalid attempt to iterate non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
    }
    var o7, a3 = true, u3 = false;
    return { s: function s4() {
      t5 = t5.call(r5);
    }, n: function n6() {
      var r6 = t5.next();
      return a3 = r6.done, r6;
    }, e: function e8(r6) {
      u3 = true, o7 = r6;
    }, f: function f3() {
      try {
        a3 || null == t5["return"] || t5["return"]();
      } finally {
        if (u3) throw o7;
      }
    } };
  }
  function _unsupportedIterableToArray$1(r5, a3) {
    if (r5) {
      if ("string" == typeof r5) return _arrayLikeToArray$1(r5, a3);
      var t5 = {}.toString.call(r5).slice(8, -1);
      return "Object" === t5 && r5.constructor && (t5 = r5.constructor.name), "Map" === t5 || "Set" === t5 ? Array.from(r5) : "Arguments" === t5 || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(t5) ? _arrayLikeToArray$1(r5, a3) : void 0;
    }
  }
  function _arrayLikeToArray$1(r5, a3) {
    (null == a3 || a3 > r5.length) && (a3 = r5.length);
    for (var e7 = 0, n6 = Array(a3); e7 < a3; e7++) n6[e7] = r5[e7];
    return n6;
  }
  function useHelper() {
    return {
      debug: function debug(_f) {
      }
    };
  }
  var Signal = /* @__PURE__ */ (function() {
    function Signal2() {
      _classCallCheck(this, Signal2);
      _defineProperty(this, "pipes", []);
    }
    return _createClass(Signal2, [{
      key: "addPipe",
      value: function addPipe(pipe) {
        this.pipes.push(pipe);
      }
    }, {
      key: "emit",
      value: (function() {
        var _emit = _asyncToGenerator(/* @__PURE__ */ import_regenerator.default.mark(function _callee(context2) {
          var current, _iterator, _step, pipe;
          return import_regenerator.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                current = context2;
                _iterator = _createForOfIteratorHelper$1(this.pipes);
                _context.prev = 2;
                _iterator.s();
              case 4:
                if ((_step = _iterator.n()).done) {
                  _context.next = 13;
                  break;
                }
                pipe = _step.value;
                _context.next = 8;
                return pipe(current);
              case 8:
                current = _context.sent;
                if (!(typeof current === "undefined")) {
                  _context.next = 11;
                  break;
                }
                return _context.abrupt("return");
              case 11:
                _context.next = 4;
                break;
              case 13:
                _context.next = 18;
                break;
              case 15:
                _context.prev = 15;
                _context.t0 = _context["catch"](2);
                _iterator.e(_context.t0);
              case 18:
                _context.prev = 18;
                _iterator.f();
                return _context.finish(18);
              case 21:
                return _context.abrupt("return", current);
              case 22:
              case "end":
                return _context.stop();
            }
          }, _callee, this, [[2, 15, 18, 21]]);
        }));
        function emit(_x) {
          return _emit.apply(this, arguments);
        }
        return emit;
      })()
    }]);
  })();
  var Scope = /* @__PURE__ */ (function() {
    function Scope2(name) {
      _classCallCheck(this, Scope2);
      _defineProperty(this, "signal", new Signal());
      this.name = name;
    }
    return _createClass(Scope2, [{
      key: "addPipe",
      value: function addPipe(middleware) {
        this.signal.addPipe(middleware);
      }
    }, {
      key: "use",
      value: function use(scope) {
        if (!(scope instanceof Scope2)) throw new Error("cannot use non-Scope instance");
        scope.setParent(this);
        this.addPipe(function(context2) {
          return scope.signal.emit(context2);
        });
        return useHelper();
      }
    }, {
      key: "setParent",
      value: function setParent(scope) {
        this.parent = scope;
      }
    }, {
      key: "emit",
      value: function emit(context2) {
        return this.signal.emit(context2);
      }
    }, {
      key: "hasParent",
      value: function hasParent() {
        return Boolean(this.parent);
      }
    }, {
      key: "parentScope",
      value: function parentScope(type) {
        if (!this.parent) throw new Error("cannot find parent");
        if (type && this.parent instanceof type) return this.parent;
        if (type) throw new Error("actual parent is not instance of type");
        return this.parent;
      }
    }]);
  })();
  function _createForOfIteratorHelper(r5, e7) {
    var t5 = "undefined" != typeof Symbol && r5[Symbol.iterator] || r5["@@iterator"];
    if (!t5) {
      if (Array.isArray(r5) || (t5 = _unsupportedIterableToArray(r5)) || e7 && r5 && "number" == typeof r5.length) {
        t5 && (r5 = t5);
        var _n = 0, F = function F2() {
        };
        return { s: F, n: function n6() {
          return _n >= r5.length ? { done: true } : { done: false, value: r5[_n++] };
        }, e: function e8(r6) {
          throw r6;
        }, f: F };
      }
      throw new TypeError("Invalid attempt to iterate non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
    }
    var o7, a3 = true, u3 = false;
    return { s: function s4() {
      t5 = t5.call(r5);
    }, n: function n6() {
      var r6 = t5.next();
      return a3 = r6.done, r6;
    }, e: function e8(r6) {
      u3 = true, o7 = r6;
    }, f: function f3() {
      try {
        a3 || null == t5["return"] || t5["return"]();
      } finally {
        if (u3) throw o7;
      }
    } };
  }
  function _unsupportedIterableToArray(r5, a3) {
    if (r5) {
      if ("string" == typeof r5) return _arrayLikeToArray(r5, a3);
      var t5 = {}.toString.call(r5).slice(8, -1);
      return "Object" === t5 && r5.constructor && (t5 = r5.constructor.name), "Map" === t5 || "Set" === t5 ? Array.from(r5) : "Arguments" === t5 || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(t5) ? _arrayLikeToArray(r5, a3) : void 0;
    }
  }
  function _arrayLikeToArray(r5, a3) {
    (null == a3 || a3 > r5.length) && (a3 = r5.length);
    for (var e7 = 0, n6 = Array(a3); e7 < a3; e7++) n6[e7] = r5[e7];
    return n6;
  }
  function _callSuper$1(t5, o7, e7) {
    return o7 = _getPrototypeOf(o7), _possibleConstructorReturn(t5, _isNativeReflectConstruct$1() ? Reflect.construct(o7, e7 || [], _getPrototypeOf(t5).constructor) : o7.apply(t5, e7));
  }
  function _isNativeReflectConstruct$1() {
    try {
      var t5 = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
    } catch (t6) {
    }
    return (_isNativeReflectConstruct$1 = function _isNativeReflectConstruct6() {
      return !!t5;
    })();
  }
  var NodeEditor = /* @__PURE__ */ (function(_Scope) {
    function NodeEditor2() {
      var _this;
      _classCallCheck(this, NodeEditor2);
      _this = _callSuper$1(this, NodeEditor2, ["NodeEditor"]);
      _defineProperty(_this, "nodes", []);
      _defineProperty(_this, "connections", []);
      return _this;
    }
    _inherits(NodeEditor2, _Scope);
    return _createClass(NodeEditor2, [{
      key: "getNode",
      value: function getNode(id) {
        return this.nodes.find(function(node) {
          return node.id === id;
        });
      }
      /**
       * Get all nodes
       * @returns Copy of array with nodes
       */
    }, {
      key: "getNodes",
      value: function getNodes() {
        return this.nodes.slice();
      }
      /**
       * Get all connections
       * @returns Copy of array with onnections
       */
    }, {
      key: "getConnections",
      value: function getConnections() {
        return this.connections.slice();
      }
      /**
       * Get a connection by id
       * @param id - The connection id
       * @returns The connection or undefined
       */
    }, {
      key: "getConnection",
      value: function getConnection(id) {
        return this.connections.find(function(connection) {
          return connection.id === id;
        });
      }
      /**
       * Add a node
       * @param data - The node data
       * @returns Whether the node was added
       * @throws If the node has already been added
       * @emits nodecreate
       * @emits nodecreated
       */
    }, {
      key: "addNode",
      value: (function() {
        var _addNode = _asyncToGenerator(/* @__PURE__ */ import_regenerator.default.mark(function _callee(data) {
          return import_regenerator.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                if (!this.getNode(data.id)) {
                  _context.next = 2;
                  break;
                }
                throw new Error("node has already been added");
              case 2:
                _context.next = 4;
                return this.emit({
                  type: "nodecreate",
                  data
                });
              case 4:
                if (_context.sent) {
                  _context.next = 6;
                  break;
                }
                return _context.abrupt("return", false);
              case 6:
                this.nodes.push(data);
                _context.next = 9;
                return this.emit({
                  type: "nodecreated",
                  data
                });
              case 9:
                return _context.abrupt("return", true);
              case 10:
              case "end":
                return _context.stop();
            }
          }, _callee, this);
        }));
        function addNode(_x) {
          return _addNode.apply(this, arguments);
        }
        return addNode;
      })()
    }, {
      key: "addConnection",
      value: (function() {
        var _addConnection = _asyncToGenerator(/* @__PURE__ */ import_regenerator.default.mark(function _callee2(data) {
          return import_regenerator.default.wrap(function _callee2$(_context2) {
            while (1) switch (_context2.prev = _context2.next) {
              case 0:
                if (!this.getConnection(data.id)) {
                  _context2.next = 2;
                  break;
                }
                throw new Error("connection has already been added");
              case 2:
                _context2.next = 4;
                return this.emit({
                  type: "connectioncreate",
                  data
                });
              case 4:
                if (_context2.sent) {
                  _context2.next = 6;
                  break;
                }
                return _context2.abrupt("return", false);
              case 6:
                this.connections.push(data);
                _context2.next = 9;
                return this.emit({
                  type: "connectioncreated",
                  data
                });
              case 9:
                return _context2.abrupt("return", true);
              case 10:
              case "end":
                return _context2.stop();
            }
          }, _callee2, this);
        }));
        function addConnection(_x2) {
          return _addConnection.apply(this, arguments);
        }
        return addConnection;
      })()
    }, {
      key: "removeNode",
      value: (function() {
        var _removeNode = _asyncToGenerator(/* @__PURE__ */ import_regenerator.default.mark(function _callee3(id) {
          var node, index4;
          return import_regenerator.default.wrap(function _callee3$(_context3) {
            while (1) switch (_context3.prev = _context3.next) {
              case 0:
                node = this.nodes.find(function(n6) {
                  return n6.id === id;
                });
                if (node) {
                  _context3.next = 3;
                  break;
                }
                throw new Error("cannot find node");
              case 3:
                _context3.next = 5;
                return this.emit({
                  type: "noderemove",
                  data: node
                });
              case 5:
                if (_context3.sent) {
                  _context3.next = 7;
                  break;
                }
                return _context3.abrupt("return", false);
              case 7:
                index4 = this.nodes.indexOf(node);
                this.nodes.splice(index4, 1);
                _context3.next = 11;
                return this.emit({
                  type: "noderemoved",
                  data: node
                });
              case 11:
                return _context3.abrupt("return", true);
              case 12:
              case "end":
                return _context3.stop();
            }
          }, _callee3, this);
        }));
        function removeNode(_x3) {
          return _removeNode.apply(this, arguments);
        }
        return removeNode;
      })()
    }, {
      key: "removeConnection",
      value: (function() {
        var _removeConnection = _asyncToGenerator(/* @__PURE__ */ import_regenerator.default.mark(function _callee4(id) {
          var connection, index4;
          return import_regenerator.default.wrap(function _callee4$(_context4) {
            while (1) switch (_context4.prev = _context4.next) {
              case 0:
                connection = this.connections.find(function(c4) {
                  return c4.id === id;
                });
                if (connection) {
                  _context4.next = 3;
                  break;
                }
                throw new Error("cannot find connection");
              case 3:
                _context4.next = 5;
                return this.emit({
                  type: "connectionremove",
                  data: connection
                });
              case 5:
                if (_context4.sent) {
                  _context4.next = 7;
                  break;
                }
                return _context4.abrupt("return", false);
              case 7:
                index4 = this.connections.indexOf(connection);
                this.connections.splice(index4, 1);
                _context4.next = 11;
                return this.emit({
                  type: "connectionremoved",
                  data: connection
                });
              case 11:
                return _context4.abrupt("return", true);
              case 12:
              case "end":
                return _context4.stop();
            }
          }, _callee4, this);
        }));
        function removeConnection(_x4) {
          return _removeConnection.apply(this, arguments);
        }
        return removeConnection;
      })()
    }, {
      key: "clear",
      value: (function() {
        var _clear = _asyncToGenerator(/* @__PURE__ */ import_regenerator.default.mark(function _callee5() {
          var _iterator, _step, connection, _iterator2, _step2, node;
          return import_regenerator.default.wrap(function _callee5$(_context5) {
            while (1) switch (_context5.prev = _context5.next) {
              case 0:
                _context5.next = 2;
                return this.emit({
                  type: "clear"
                });
              case 2:
                if (_context5.sent) {
                  _context5.next = 6;
                  break;
                }
                _context5.next = 5;
                return this.emit({
                  type: "clearcancelled"
                });
              case 5:
                return _context5.abrupt("return", false);
              case 6:
                _iterator = _createForOfIteratorHelper(this.connections.slice());
                _context5.prev = 7;
                _iterator.s();
              case 9:
                if ((_step = _iterator.n()).done) {
                  _context5.next = 15;
                  break;
                }
                connection = _step.value;
                _context5.next = 13;
                return this.removeConnection(connection.id);
              case 13:
                _context5.next = 9;
                break;
              case 15:
                _context5.next = 20;
                break;
              case 17:
                _context5.prev = 17;
                _context5.t0 = _context5["catch"](7);
                _iterator.e(_context5.t0);
              case 20:
                _context5.prev = 20;
                _iterator.f();
                return _context5.finish(20);
              case 23:
                _iterator2 = _createForOfIteratorHelper(this.nodes.slice());
                _context5.prev = 24;
                _iterator2.s();
              case 26:
                if ((_step2 = _iterator2.n()).done) {
                  _context5.next = 32;
                  break;
                }
                node = _step2.value;
                _context5.next = 30;
                return this.removeNode(node.id);
              case 30:
                _context5.next = 26;
                break;
              case 32:
                _context5.next = 37;
                break;
              case 34:
                _context5.prev = 34;
                _context5.t1 = _context5["catch"](24);
                _iterator2.e(_context5.t1);
              case 37:
                _context5.prev = 37;
                _iterator2.f();
                return _context5.finish(37);
              case 40:
                _context5.next = 42;
                return this.emit({
                  type: "cleared"
                });
              case 42:
                return _context5.abrupt("return", true);
              case 43:
              case "end":
                return _context5.stop();
            }
          }, _callee5, this, [[7, 17, 20, 23], [24, 34, 37, 40]]);
        }));
        function clear() {
          return _clear.apply(this, arguments);
        }
        return clear;
      })()
    }]);
  })(Scope);
  var crypto = globalThis.crypto;
  function getUID() {
    if ("randomBytes" in crypto) {
      return crypto.randomBytes(8).toString("hex");
    }
    var bytes = crypto.getRandomValues(new Uint8Array(8));
    var array = Array.from(bytes);
    var hexPairs = array.map(function(b3) {
      return b3.toString(16).padStart(2, "0");
    });
    return hexPairs.join("");
  }
  function _callSuper(t5, o7, e7) {
    return o7 = _getPrototypeOf(o7), _possibleConstructorReturn(t5, _isNativeReflectConstruct() ? Reflect.construct(o7, e7 || [], _getPrototypeOf(t5).constructor) : o7.apply(t5, e7));
  }
  function _isNativeReflectConstruct() {
    try {
      var t5 = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
    } catch (t6) {
    }
    return (_isNativeReflectConstruct = function _isNativeReflectConstruct6() {
      return !!t5;
    })();
  }
  var Socket = /* @__PURE__ */ _createClass(
    /**
     * @constructor
     * @param name Name of the socket
     */
    function Socket2(name) {
      _classCallCheck(this, Socket2);
      this.name = name;
    }
  );
  var Port = /* @__PURE__ */ _createClass(
    /**
     * Port id, unique string generated by `getUID` function
     */
    /**
     * Port index, used for sorting ports. Default is `0`
     */
    /**
     * @constructor
     * @param socket Socket instance
     * @param label Label of the port
     * @param multipleConnections Whether the output port can have multiple connections
     */
    function Port2(socket, label, multipleConnections) {
      _classCallCheck(this, Port2);
      this.socket = socket;
      this.label = label;
      this.multipleConnections = multipleConnections;
      this.id = getUID();
    }
  );
  var Input = /* @__PURE__ */ (function(_Port) {
    function Input2(socket, label, multipleConnections) {
      var _this;
      _classCallCheck(this, Input2);
      _this = _callSuper(this, Input2, [socket, label, multipleConnections]);
      _defineProperty(_this, "control", null);
      _defineProperty(_this, "showControl", true);
      _this.socket = socket;
      _this.label = label;
      _this.multipleConnections = multipleConnections;
      return _this;
    }
    _inherits(Input2, _Port);
    return _createClass(Input2, [{
      key: "addControl",
      value: function addControl(control) {
        if (this.control) throw new Error("control already added for this input");
        this.control = control;
      }
      /**
       * Remove control from the input port
       */
    }, {
      key: "removeControl",
      value: function removeControl() {
        this.control = null;
      }
    }]);
  })(Port);
  var Output = /* @__PURE__ */ (function(_Port2) {
    function Output2(socket, label, multipleConnections) {
      _classCallCheck(this, Output2);
      return _callSuper(this, Output2, [socket, label, multipleConnections !== false]);
    }
    _inherits(Output2, _Port2);
    return _createClass(Output2);
  })(Port);
  var Control = /* @__PURE__ */ _createClass(
    /**
     * Control id, unique string generated by `getUID` function
     */
    /**
     * Control index, used for sorting controls. Default is `0`
     */
    function Control2() {
      _classCallCheck(this, Control2);
      this.id = getUID();
    }
  );
  var InputControl = /* @__PURE__ */ (function(_Control) {
    function InputControl2(type, options) {
      var _options$readonly;
      var _this2;
      _classCallCheck(this, InputControl2);
      _this2 = _callSuper(this, InputControl2);
      _this2.type = type;
      _this2.options = options;
      _this2.id = getUID();
      _this2.readonly = (_options$readonly = options === null || options === void 0 ? void 0 : options.readonly) !== null && _options$readonly !== void 0 ? _options$readonly : false;
      if (typeof (options === null || options === void 0 ? void 0 : options.initial) !== "undefined") _this2.value = options.initial;
      return _this2;
    }
    _inherits(InputControl2, _Control);
    return _createClass(InputControl2, [{
      key: "setValue",
      value: function setValue(value) {
        var _this$options;
        this.value = value;
        if ((_this$options = this.options) !== null && _this$options !== void 0 && _this$options.change) this.options.change(value);
      }
    }]);
  })(Control);
  var Node = /* @__PURE__ */ (function() {
    function Node3(label) {
      _classCallCheck(this, Node3);
      _defineProperty(this, "inputs", {});
      _defineProperty(this, "outputs", {});
      _defineProperty(this, "controls", {});
      this.label = label;
      this.id = getUID();
    }
    return _createClass(Node3, [{
      key: "hasInput",
      value: function hasInput(key) {
        return Object.prototype.hasOwnProperty.call(this.inputs, key);
      }
    }, {
      key: "addInput",
      value: function addInput(key, input) {
        if (this.hasInput(key)) throw new Error("input with key '".concat(String(key), "' already added"));
        Object.defineProperty(this.inputs, key, {
          value: input,
          enumerable: true,
          configurable: true
        });
      }
    }, {
      key: "removeInput",
      value: function removeInput(key) {
        delete this.inputs[key];
      }
    }, {
      key: "hasOutput",
      value: function hasOutput(key) {
        return Object.prototype.hasOwnProperty.call(this.outputs, key);
      }
    }, {
      key: "addOutput",
      value: function addOutput(key, output) {
        if (this.hasOutput(key)) throw new Error("output with key '".concat(String(key), "' already added"));
        Object.defineProperty(this.outputs, key, {
          value: output,
          enumerable: true,
          configurable: true
        });
      }
    }, {
      key: "removeOutput",
      value: function removeOutput(key) {
        delete this.outputs[key];
      }
    }, {
      key: "hasControl",
      value: function hasControl(key) {
        return Object.prototype.hasOwnProperty.call(this.controls, key);
      }
    }, {
      key: "addControl",
      value: function addControl(key, control) {
        if (this.hasControl(key)) throw new Error("control with key '".concat(String(key), "' already added"));
        Object.defineProperty(this.controls, key, {
          value: control,
          enumerable: true,
          configurable: true
        });
      }
    }, {
      key: "removeControl",
      value: function removeControl(key) {
        delete this.controls[key];
      }
    }]);
  })();
  var Connection = /* @__PURE__ */ _createClass(
    /**
     * Connection id, unique string generated by `getUID` function
     */
    /**
     * Source node id
     */
    /**
     * Target node id
     */
    /**
     * @constructor
     * @param source Source node instance
     * @param sourceOutput Source node output key
     * @param target Target node instance
     * @param targetInput Target node input key
     */
    function Connection2(source, sourceOutput, target, targetInput) {
      _classCallCheck(this, Connection2);
      this.sourceOutput = sourceOutput;
      this.targetInput = targetInput;
      if (!source.outputs[sourceOutput]) {
        throw new Error("source node doesn't have output with a key ".concat(String(sourceOutput)));
      }
      if (!target.inputs[targetInput]) {
        throw new Error("target node doesn't have input with a key ".concat(String(targetInput)));
      }
      this.id = getUID();
      this.source = source.id;
      this.target = target.id;
    }
  );
  var classic = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    Socket,
    Port,
    Input,
    Output,
    Control,
    InputControl,
    Node,
    Connection
  });

  // node_modules/rete-area-plugin/rete-area-plugin.esm.js
  var import_regenerator2 = __toESM(require_regenerator2());

  // node_modules/@babel/runtime/helpers/esm/arrayLikeToArray.js
  function _arrayLikeToArray2(r5, a3) {
    (null == a3 || a3 > r5.length) && (a3 = r5.length);
    for (var e7 = 0, n6 = Array(a3); e7 < a3; e7++) n6[e7] = r5[e7];
    return n6;
  }

  // node_modules/@babel/runtime/helpers/esm/arrayWithoutHoles.js
  function _arrayWithoutHoles(r5) {
    if (Array.isArray(r5)) return _arrayLikeToArray2(r5);
  }

  // node_modules/@babel/runtime/helpers/esm/iterableToArray.js
  function _iterableToArray(r5) {
    if ("undefined" != typeof Symbol && null != r5[Symbol.iterator] || null != r5["@@iterator"]) return Array.from(r5);
  }

  // node_modules/@babel/runtime/helpers/esm/unsupportedIterableToArray.js
  function _unsupportedIterableToArray2(r5, a3) {
    if (r5) {
      if ("string" == typeof r5) return _arrayLikeToArray2(r5, a3);
      var t5 = {}.toString.call(r5).slice(8, -1);
      return "Object" === t5 && r5.constructor && (t5 = r5.constructor.name), "Map" === t5 || "Set" === t5 ? Array.from(r5) : "Arguments" === t5 || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(t5) ? _arrayLikeToArray2(r5, a3) : void 0;
    }
  }

  // node_modules/@babel/runtime/helpers/esm/nonIterableSpread.js
  function _nonIterableSpread() {
    throw new TypeError("Invalid attempt to spread non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
  }

  // node_modules/@babel/runtime/helpers/esm/toConsumableArray.js
  function _toConsumableArray(r5) {
    return _arrayWithoutHoles(r5) || _iterableToArray(r5) || _unsupportedIterableToArray2(r5) || _nonIterableSpread();
  }

  // node_modules/rete-area-plugin/rete-area-plugin.esm.js
  var Content = /* @__PURE__ */ (function() {
    function Content2(reordered) {
      _classCallCheck(this, Content2);
      this.reordered = reordered;
      this.holder = document.createElement("div");
      this.holder.style.transformOrigin = "0 0";
    }
    return _createClass(Content2, [{
      key: "getPointerFrom",
      value: function getPointerFrom(event) {
        var _this$holder$getBound = this.holder.getBoundingClientRect(), left = _this$holder$getBound.left, top = _this$holder$getBound.top;
        var x2 = event.clientX - left;
        var y3 = event.clientY - top;
        return {
          x: x2,
          y: y3
        };
      }
    }, {
      key: "add",
      value: function add(element) {
        this.holder.appendChild(element);
      }
    }, {
      key: "reorder",
      value: (function() {
        var _reorder = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee(target, next) {
          return import_regenerator2.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                if (this.holder.contains(target)) {
                  _context.next = 2;
                  break;
                }
                throw new Error("content doesn't have 'target' for reordering");
              case 2:
                if (!(next !== null && !this.holder.contains(next))) {
                  _context.next = 4;
                  break;
                }
                throw new Error("content doesn't have 'next' for reordering");
              case 4:
                this.holder.insertBefore(target, next);
                _context.next = 7;
                return this.reordered(target);
              case 7:
              case "end":
                return _context.stop();
            }
          }, _callee, this);
        }));
        function reorder(_x, _x2) {
          return _reorder.apply(this, arguments);
        }
        return reorder;
      })()
    }, {
      key: "remove",
      value: function remove(element) {
        if (this.holder.contains(element)) {
          this.holder.removeChild(element);
        }
      }
    }]);
  })();
  function usePointerListener(element, handlers) {
    var move = function move2(event) {
      handlers.move(event);
    };
    var _up = function up(event) {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", _up);
      window.removeEventListener("pointercancel", _up);
      handlers.up(event);
    };
    var down = function down2(event) {
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", _up);
      window.addEventListener("pointercancel", _up);
      handlers.down(event);
    };
    element.addEventListener("pointerdown", down);
    return {
      destroy: function destroy2() {
        element.removeEventListener("pointerdown", down);
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", _up);
        window.removeEventListener("pointercancel", _up);
      }
    };
  }
  var min = function min2(arr) {
    return arr.length === 0 ? 0 : Math.min.apply(Math, _toConsumableArray(arr));
  };
  var max = function max2(arr) {
    return arr.length === 0 ? 0 : Math.max.apply(Math, _toConsumableArray(arr));
  };
  function getBoundingBox$1(rects) {
    var left = min(rects.map(function(rect) {
      return rect.position.x;
    }));
    var top = min(rects.map(function(rect) {
      return rect.position.y;
    }));
    var right = max(rects.map(function(rect) {
      return rect.position.x + rect.width;
    }));
    var bottom = max(rects.map(function(rect) {
      return rect.position.y + rect.height;
    }));
    return {
      left,
      right,
      top,
      bottom,
      width: Math.abs(left - right),
      height: Math.abs(top - bottom),
      center: {
        x: (left + right) / 2,
        y: (top + bottom) / 2
      }
    };
  }
  function ownKeys$4(e7, r5) {
    var t5 = Object.keys(e7);
    if (Object.getOwnPropertySymbols) {
      var o7 = Object.getOwnPropertySymbols(e7);
      r5 && (o7 = o7.filter(function(r6) {
        return Object.getOwnPropertyDescriptor(e7, r6).enumerable;
      })), t5.push.apply(t5, o7);
    }
    return t5;
  }
  function _objectSpread$4(e7) {
    for (var r5 = 1; r5 < arguments.length; r5++) {
      var t5 = null != arguments[r5] ? arguments[r5] : {};
      r5 % 2 ? ownKeys$4(Object(t5), true).forEach(function(r6) {
        _defineProperty(e7, r6, t5[r6]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e7, Object.getOwnPropertyDescriptors(t5)) : ownKeys$4(Object(t5)).forEach(function(r6) {
        Object.defineProperty(e7, r6, Object.getOwnPropertyDescriptor(t5, r6));
      });
    }
    return e7;
  }
  var Drag = /* @__PURE__ */ (function() {
    function Drag2(guards) {
      var _this = this;
      _classCallCheck(this, Drag2);
      _defineProperty(this, "down", function(e7) {
        if (!_this.guards.down(e7)) return;
        e7.stopPropagation();
        _this.pointerStart = {
          x: e7.pageX,
          y: e7.pageY
        };
        _this.startPosition = _objectSpread$4({}, _this.config.getCurrentPosition());
        _this.events.start(e7);
      });
      _defineProperty(this, "move", function(e7) {
        if (!_this.pointerStart || !_this.startPosition) return;
        if (!_this.guards.move(e7)) return;
        e7.preventDefault();
        var delta = {
          x: e7.pageX - _this.pointerStart.x,
          y: e7.pageY - _this.pointerStart.y
        };
        var zoom = _this.config.getZoom();
        var x2 = _this.startPosition.x + delta.x / zoom;
        var y3 = _this.startPosition.y + delta.y / zoom;
        void _this.events.translate(x2, y3, e7);
      });
      _defineProperty(this, "up", function(e7) {
        if (!_this.pointerStart) return;
        delete _this.pointerStart;
        _this.events.drag(e7);
      });
      this.guards = guards || {
        down: function down(e7) {
          return !(e7.pointerType === "mouse" && e7.button !== 0);
        },
        move: function move() {
          return true;
        }
      };
    }
    return _createClass(Drag2, [{
      key: "initialize",
      value: function initialize(element, config2, events) {
        this.config = config2;
        this.events = events;
        element.style.touchAction = "none";
        this.pointerListener = usePointerListener(element, {
          down: this.down,
          move: this.move,
          up: this.up
        });
      }
    }, {
      key: "destroy",
      value: function destroy2() {
        this.pointerListener.destroy();
      }
    }]);
  })();
  var LINE_HEIGHT_PX = 8;
  var PAGE_HEIGHT_PX = 24;
  var DOM_DELTA_LINE = 1;
  var DOM_DELTA_PAGE = 2;
  function wheelDeltaToPixels(delta, mode) {
    if (mode === DOM_DELTA_LINE) return delta * LINE_HEIGHT_PX;
    if (mode === DOM_DELTA_PAGE) return delta * PAGE_HEIGHT_PX;
    return delta;
  }
  function computeWheelZoomDelta(pixels, intensity) {
    if (pixels === 0) return 0;
    var unit = intensity / LINE_HEIGHT_PX;
    var raw = -pixels * unit;
    return Math.sign(raw) * Math.min(intensity, Math.abs(raw));
  }
  var Zoom = /* @__PURE__ */ (function() {
    function Zoom2(intensity) {
      var _this = this;
      _classCallCheck(this, Zoom2);
      _defineProperty(this, "previous", null);
      _defineProperty(this, "pointers", []);
      _defineProperty(this, "wheel", function(e7) {
        e7.preventDefault();
        var pixels = wheelDeltaToPixels(e7.deltaY, e7.deltaMode);
        var delta = computeWheelZoomDelta(pixels, _this.intensity);
        if (delta === 0) return;
        var _this$element$getBoun = _this.element.getBoundingClientRect(), left = _this$element$getBoun.left, top = _this$element$getBoun.top;
        var ox = (left - e7.clientX) * delta;
        var oy = (top - e7.clientY) * delta;
        _this.onzoom(delta, ox, oy, "wheel");
      });
      _defineProperty(this, "down", function(e7) {
        _this.pointers.push(e7);
      });
      _defineProperty(this, "move", function(e7) {
        _this.pointers = _this.pointers.map(function(p3) {
          return p3.pointerId === e7.pointerId ? e7 : p3;
        });
        if (!_this.isTranslating()) return;
        var _this$element$getBoun2 = _this.element.getBoundingClientRect(), left = _this$element$getBoun2.left, top = _this$element$getBoun2.top;
        var _this$getTouches = _this.getTouches(), cx = _this$getTouches.cx, cy = _this$getTouches.cy, distance = _this$getTouches.distance;
        if (_this.previous !== null && _this.previous.distance > 0) {
          var _delta = distance / _this.previous.distance - 1;
          var _ox = (left - cx) * _delta;
          var _oy = (top - cy) * _delta;
          _this.onzoom(_delta, _ox - (_this.previous.cx - cx), _oy - (_this.previous.cy - cy), "touch");
        }
        _this.previous = {
          cx,
          cy,
          distance
        };
      });
      _defineProperty(this, "contextmenu", function() {
        _this.pointers = [];
      });
      _defineProperty(this, "up", function(e7) {
        _this.previous = null;
        _this.pointers = _this.pointers.filter(function(p3) {
          return p3.pointerId !== e7.pointerId;
        });
      });
      _defineProperty(this, "dblclick", function(e7) {
        e7.preventDefault();
        var _this$element$getBoun3 = _this.element.getBoundingClientRect(), left = _this$element$getBoun3.left, top = _this$element$getBoun3.top;
        var delta = 4 * _this.intensity;
        var ox = (left - e7.clientX) * delta;
        var oy = (top - e7.clientY) * delta;
        _this.onzoom(delta, ox, oy, "dblclick");
      });
      this.intensity = intensity;
    }
    return _createClass(Zoom2, [{
      key: "initialize",
      value: function initialize(container, element, onzoom) {
        this.container = container;
        this.element = element;
        this.onzoom = onzoom;
        this.container.addEventListener("wheel", this.wheel);
        this.container.addEventListener("pointerdown", this.down);
        this.container.addEventListener("dblclick", this.dblclick);
        window.addEventListener("pointermove", this.move);
        window.addEventListener("pointerup", this.up);
        window.addEventListener("pointercancel", this.up);
        window.addEventListener("contextmenu", this.contextmenu);
      }
    }, {
      key: "getTouches",
      value: function getTouches() {
        var e7 = {
          touches: this.pointers
        };
        var _ref = [e7.touches[0].clientX, e7.touches[0].clientY], x1 = _ref[0], y1 = _ref[1];
        var _ref2 = [e7.touches[1].clientX, e7.touches[1].clientY], x2 = _ref2[0], y22 = _ref2[1];
        var distance = Math.sqrt(Math.pow(x1 - x2, 2) + Math.pow(y1 - y22, 2));
        return {
          cx: (x1 + x2) / 2,
          cy: (y1 + y22) / 2,
          distance
        };
      }
    }, {
      key: "isTranslating",
      value: function isTranslating() {
        return this.pointers.length >= 2;
      }
    }, {
      key: "destroy",
      value: function destroy2() {
        this.container.removeEventListener("wheel", this.wheel);
        this.container.removeEventListener("pointerdown", this.down);
        this.container.removeEventListener("dblclick", this.dblclick);
        window.removeEventListener("pointermove", this.move);
        window.removeEventListener("pointerup", this.up);
        window.removeEventListener("pointercancel", this.up);
        window.removeEventListener("contextmenu", this.contextmenu);
      }
    }]);
  })();
  var Area = /* @__PURE__ */ (function() {
    function Area2(container, events, guards) {
      var _this = this;
      _classCallCheck(this, Area2);
      _defineProperty(this, "transform", {
        k: 1,
        x: 0,
        y: 0
      });
      _defineProperty(this, "pointer", {
        x: 0,
        y: 0
      });
      _defineProperty(this, "zoomHandler", null);
      _defineProperty(this, "dragHandler", null);
      _defineProperty(this, "pointerdown", function(event) {
        _this.setPointerFrom(event);
        _this.events.pointerDown(_this.pointer, event);
      });
      _defineProperty(this, "pointermove", function(event) {
        _this.setPointerFrom(event);
        _this.events.pointerMove(_this.pointer, event);
      });
      _defineProperty(this, "pointerup", function(event) {
        _this.setPointerFrom(event);
        _this.events.pointerUp(_this.pointer, event);
      });
      _defineProperty(this, "resize", function(event) {
        _this.events.resize(event);
      });
      _defineProperty(this, "onTranslate", function(x2, y3) {
        var _this$zoomHandler;
        if ((_this$zoomHandler = _this.zoomHandler) !== null && _this$zoomHandler !== void 0 && _this$zoomHandler.isTranslating()) return;
        void _this.translate(x2, y3);
      });
      _defineProperty(this, "onZoom", function(delta, ox, oy, source) {
        void _this.zoom(_this.transform.k * (1 + delta), ox, oy, source);
        _this.update();
      });
      this.container = container;
      this.events = events;
      this.guards = guards;
      this.content = new Content(function(element) {
        return _this.events.reordered(element);
      });
      this.content.holder.style.transformOrigin = "0 0";
      this.setZoomHandler(new Zoom(0.1));
      this.setDragHandler(new Drag());
      this.container.addEventListener("pointerdown", this.pointerdown);
      this.container.addEventListener("pointermove", this.pointermove);
      window.addEventListener("pointerup", this.pointerup);
      window.addEventListener("resize", this.resize);
      container.appendChild(this.content.holder);
      this.update();
    }
    return _createClass(Area2, [{
      key: "update",
      value: function update() {
        var _this$transform = this.transform, x2 = _this$transform.x, y3 = _this$transform.y, k2 = _this$transform.k;
        this.content.holder.style.transform = "translate(".concat(x2, "px, ").concat(y3, "px) scale(").concat(k2, ")");
      }
      /**
       * Drag handler. Destroy previous drag handler if exists.
       * @param drag drag handler
       * @example area.area.setDragHandler(null) // disable drag
       */
    }, {
      key: "setDragHandler",
      value: function setDragHandler(drag) {
        var _this2 = this;
        if (this.dragHandler) this.dragHandler.destroy();
        this.dragHandler = drag;
        if (this.dragHandler) this.dragHandler.initialize(this.container, {
          getCurrentPosition: function getCurrentPosition() {
            return _this2.transform;
          },
          getZoom: function getZoom() {
            return 1;
          }
        }, {
          start: function start() {
            return null;
          },
          translate: this.onTranslate,
          drag: function drag2() {
            return null;
          }
        });
      }
      /**
       * Set zoom handler. Destroy previous zoom handler if exists.
       * @param zoom zoom handler
       * @example area.area.setZoomHandler(null) // disable zoom
       */
    }, {
      key: "setZoomHandler",
      value: function setZoomHandler(zoom) {
        if (this.zoomHandler) this.zoomHandler.destroy();
        this.zoomHandler = zoom;
        if (this.zoomHandler) this.zoomHandler.initialize(this.container, this.content.holder, this.onZoom);
      }
    }, {
      key: "setPointerFrom",
      value: function setPointerFrom(event) {
        var _this$content$getPoin = this.content.getPointerFrom(event), x2 = _this$content$getPoin.x, y3 = _this$content$getPoin.y;
        var k2 = this.transform.k;
        this.pointer = {
          x: x2 / k2,
          y: y3 / k2
        };
      }
    }, {
      key: "translate",
      value: (
        /**
         * Change position of the area
         * @param x desired x coordinate
         * @param y desired y coordinate
         * @returns true if the translation was successful, false otherwise
         * @emits translate
         * @emits translated
         */
        (function() {
          var _translate = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee(x2, y3) {
            var position, result;
            return import_regenerator2.default.wrap(function _callee$(_context) {
              while (1) switch (_context.prev = _context.next) {
                case 0:
                  position = {
                    x: x2,
                    y: y3
                  };
                  _context.next = 3;
                  return this.guards.translate({
                    previous: this.transform,
                    position
                  });
                case 3:
                  result = _context.sent;
                  if (result) {
                    _context.next = 6;
                    break;
                  }
                  return _context.abrupt("return", false);
                case 6:
                  this.transform.x = result.data.position.x;
                  this.transform.y = result.data.position.y;
                  this.update();
                  _context.next = 11;
                  return this.events.translated(result.data);
                case 11:
                  return _context.abrupt("return", true);
                case 12:
                case "end":
                  return _context.stop();
              }
            }, _callee, this);
          }));
          function translate(_x, _x2) {
            return _translate.apply(this, arguments);
          }
          return translate;
        })()
      )
    }, {
      key: "zoom",
      value: (function() {
        var _zoom2 = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee2(_zoom) {
          var ox, oy, source, k2, result, d3, _args2 = arguments;
          return import_regenerator2.default.wrap(function _callee2$(_context2) {
            while (1) switch (_context2.prev = _context2.next) {
              case 0:
                ox = _args2.length > 1 && _args2[1] !== void 0 ? _args2[1] : 0;
                oy = _args2.length > 2 && _args2[2] !== void 0 ? _args2[2] : 0;
                source = _args2.length > 3 ? _args2[3] : void 0;
                k2 = this.transform.k;
                _context2.next = 6;
                return this.guards.zoom({
                  previous: this.transform,
                  zoom: _zoom,
                  source
                });
              case 6:
                result = _context2.sent;
                if (result) {
                  _context2.next = 9;
                  break;
                }
                return _context2.abrupt("return", true);
              case 9:
                d3 = (k2 - result.data.zoom) / (k2 - _zoom || 1);
                this.transform.k = result.data.zoom || 1;
                this.transform.x += ox * d3;
                this.transform.y += oy * d3;
                this.update();
                _context2.next = 16;
                return this.events.zoomed(result.data);
              case 16:
                return _context2.abrupt("return", false);
              case 17:
              case "end":
                return _context2.stop();
            }
          }, _callee2, this);
        }));
        function zoom(_x3) {
          return _zoom2.apply(this, arguments);
        }
        return zoom;
      })()
    }, {
      key: "destroy",
      value: function destroy2() {
        this.container.removeEventListener("pointerdown", this.pointerdown);
        this.container.removeEventListener("pointermove", this.pointermove);
        window.removeEventListener("pointerup", this.pointerup);
        window.removeEventListener("resize", this.resize);
        if (this.dragHandler) this.dragHandler.destroy();
        if (this.zoomHandler) this.zoomHandler.destroy();
        this.content.holder.innerHTML = "";
      }
    }]);
  })();
  function _callSuper$12(t5, o7, e7) {
    return o7 = _getPrototypeOf(o7), _possibleConstructorReturn(t5, _isNativeReflectConstruct$12() ? Reflect.construct(o7, e7 || [], _getPrototypeOf(t5).constructor) : o7.apply(t5, e7));
  }
  function _isNativeReflectConstruct$12() {
    try {
      var t5 = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
    } catch (t6) {
    }
    return (_isNativeReflectConstruct$12 = function _isNativeReflectConstruct6() {
      return !!t5;
    })();
  }
  var BaseAreaPlugin = /* @__PURE__ */ (function(_Scope) {
    function BaseAreaPlugin2() {
      _classCallCheck(this, BaseAreaPlugin2);
      return _callSuper$12(this, BaseAreaPlugin2, arguments);
    }
    _inherits(BaseAreaPlugin2, _Scope);
    return _createClass(BaseAreaPlugin2);
  })(Scope);
  var ConnectionView = /* @__PURE__ */ _createClass(function ConnectionView2(events) {
    _classCallCheck(this, ConnectionView2);
    this.element = document.createElement("div");
    this.element.style.position = "absolute";
    this.element.style.left = "0";
    this.element.style.top = "0";
    this.element.addEventListener("contextmenu", function(event) {
      events.contextmenu(event);
    });
  });
  var ElementsHolder = /* @__PURE__ */ (function() {
    function ElementsHolder2() {
      _classCallCheck(this, ElementsHolder2);
      _defineProperty(this, "views", /* @__PURE__ */ new WeakMap());
      _defineProperty(this, "viewsElements", /* @__PURE__ */ new Map());
    }
    return _createClass(ElementsHolder2, [{
      key: "set",
      value: function set(context2) {
        var element = context2.element, type = context2.type, payload = context2.payload;
        if (payload !== null && payload !== void 0 && payload.id) {
          this.views.set(element, context2);
          this.viewsElements.set("".concat(type, "_").concat(payload.id), element);
        }
      }
    }, {
      key: "get",
      value: function get(type, id) {
        var element = this.viewsElements.get("".concat(type, "_").concat(id));
        return element && this.views.get(element);
      }
    }, {
      key: "delete",
      value: function _delete(element) {
        var _view$payload;
        var view = this.views.get(element);
        if (view && (_view$payload = view.payload) !== null && _view$payload !== void 0 && _view$payload.id) {
          this.views["delete"](element);
          this.viewsElements["delete"]("".concat(view.type, "_").concat(view.payload.id));
        }
      }
    }]);
  })();
  function ownKeys$3(e7, r5) {
    var t5 = Object.keys(e7);
    if (Object.getOwnPropertySymbols) {
      var o7 = Object.getOwnPropertySymbols(e7);
      r5 && (o7 = o7.filter(function(r6) {
        return Object.getOwnPropertyDescriptor(e7, r6).enumerable;
      })), t5.push.apply(t5, o7);
    }
    return t5;
  }
  function _objectSpread$3(e7) {
    for (var r5 = 1; r5 < arguments.length; r5++) {
      var t5 = null != arguments[r5] ? arguments[r5] : {};
      r5 % 2 ? ownKeys$3(Object(t5), true).forEach(function(r6) {
        _defineProperty(e7, r6, t5[r6]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e7, Object.getOwnPropertyDescriptors(t5)) : ownKeys$3(Object(t5)).forEach(function(r6) {
        Object.defineProperty(e7, r6, Object.getOwnPropertyDescriptor(t5, r6));
      });
    }
    return e7;
  }
  var NodeView = /* @__PURE__ */ (function() {
    function NodeView2(getZoom, events, guards) {
      var _this = this;
      _classCallCheck(this, NodeView2);
      _defineProperty(this, "translate", /* @__PURE__ */ (function() {
        var _ref = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee(x2, y3) {
          var previous, translation;
          return import_regenerator2.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                previous = _objectSpread$3({}, _this.position);
                _context.next = 3;
                return _this.guards.translate({
                  previous,
                  position: {
                    x: x2,
                    y: y3
                  }
                });
              case 3:
                translation = _context.sent;
                if (translation) {
                  _context.next = 6;
                  break;
                }
                return _context.abrupt("return", false);
              case 6:
                _this.position = _objectSpread$3({}, translation.data.position);
                _this.element.style.transform = "translate(".concat(_this.position.x, "px, ").concat(_this.position.y, "px)");
                _context.next = 10;
                return _this.events.translated({
                  position: _this.position,
                  previous
                });
              case 10:
                return _context.abrupt("return", true);
              case 11:
              case "end":
                return _context.stop();
            }
          }, _callee);
        }));
        return function(_x, _x2) {
          return _ref.apply(this, arguments);
        };
      })());
      _defineProperty(this, "resize", /* @__PURE__ */ (function() {
        var _ref2 = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee2(width, height) {
          var size, el;
          return import_regenerator2.default.wrap(function _callee2$(_context2) {
            while (1) switch (_context2.prev = _context2.next) {
              case 0:
                size = {
                  width,
                  height
                };
                _context2.next = 3;
                return _this.guards.resize({
                  size
                });
              case 3:
                if (_context2.sent) {
                  _context2.next = 5;
                  break;
                }
                return _context2.abrupt("return", false);
              case 5:
                el = _this.element.querySelector("*:not(span):not([fragment])");
                if (!(!el || !(el instanceof HTMLElement))) {
                  _context2.next = 8;
                  break;
                }
                return _context2.abrupt("return", false);
              case 8:
                el.style.width = "".concat(width, "px");
                el.style.height = "".concat(height, "px");
                _context2.next = 12;
                return _this.events.resized({
                  size
                });
              case 12:
                return _context2.abrupt("return", true);
              case 13:
              case "end":
                return _context2.stop();
            }
          }, _callee2);
        }));
        return function(_x3, _x4) {
          return _ref2.apply(this, arguments);
        };
      })());
      this.getZoom = getZoom;
      this.events = events;
      this.guards = guards;
      this.element = document.createElement("div");
      this.element.style.position = "absolute";
      this.position = {
        x: 0,
        y: 0
      };
      void this.translate(0, 0);
      this.element.addEventListener("contextmenu", function(event) {
        _this.events.contextmenu(event);
      });
      this.dragHandler = new Drag();
      this.dragHandler.initialize(this.element, {
        getCurrentPosition: function getCurrentPosition() {
          return _this.position;
        },
        getZoom: function getZoom2() {
          return _this.getZoom();
        }
      }, {
        start: this.events.picked,
        translate: this.translate,
        drag: this.events.dragged
      });
    }
    return _createClass(NodeView2, [{
      key: "destroy",
      value: function destroy2() {
        this.dragHandler.destroy();
      }
    }]);
  })();
  function getNodesRect(nodes, views) {
    return nodes.map(function(node) {
      return {
        view: views.get(node.id),
        node
      };
    }).filter(function(item) {
      return item.view;
    }).map(function(_ref) {
      var view = _ref.view, node = _ref.node;
      var width = node.width, height = node.height;
      if (typeof width !== "undefined" && typeof height !== "undefined") {
        return {
          position: view.position,
          width,
          height
        };
      }
      return {
        position: view.position,
        width: view.element.clientWidth,
        height: view.element.clientHeight
      };
    });
  }
  function getBoundingBox(plugin, nodes) {
    var editor = plugin.parentScope(NodeEditor);
    var list = nodes.map(function(node) {
      return _typeof(node) === "object" ? node : editor.getNode(node);
    });
    var rects = getNodesRect(list, plugin.nodeViews);
    return getBoundingBox$1(rects);
  }
  function simpleNodesOrder(base) {
    var area = base;
    area.addPipe(function(context2) {
      if (!context2 || _typeof(context2) !== "object" || !("type" in context2)) return context2;
      if (context2.type === "nodepicked") {
        var view = area.nodeViews.get(context2.data.id);
        var content = area.area.content;
        if (view) {
          content.reorder(view.element, null);
        }
      }
      if (context2.type === "connectioncreated") {
        var _view = area.connectionViews.get(context2.data.id);
        var _content = area.area.content;
        if (_view) {
          _content.reorder(_view.element, _content.holder.firstChild);
        }
      }
      return context2;
    });
  }
  var CONNECTION_Z_INDEX = 0;
  var NODE_BASE_Z_INDEX = 1;
  function zIndexNodesOrder(base) {
    var area = base;
    var nodeZIndex = NODE_BASE_Z_INDEX;
    var setNodeBaseZIndex = function setNodeBaseZIndex2(id) {
      var view = area.nodeViews.get(id);
      if (view) {
        view.element.style.zIndex = String(NODE_BASE_Z_INDEX);
      }
    };
    var bringNodeToFront = function bringNodeToFront2(id) {
      var view = area.nodeViews.get(id);
      if (view) {
        nodeZIndex += 1;
        view.element.style.zIndex = String(nodeZIndex);
      }
    };
    var setConnectionBaseZIndex = function setConnectionBaseZIndex2(id) {
      var view = area.connectionViews.get(id);
      if (view) {
        view.element.style.zIndex = String(CONNECTION_Z_INDEX);
      }
    };
    area.addPipe(function(context2) {
      if (!context2 || _typeof(context2) !== "object" || !("type" in context2)) return context2;
      if (context2.type === "nodecreated") {
        setNodeBaseZIndex(context2.data.id);
      }
      if (context2.type === "nodepicked") {
        bringNodeToFront(context2.data.id);
      }
      if (context2.type === "connectioncreated") {
        setConnectionBaseZIndex(context2.data.id);
      }
      return context2;
    });
  }
  function ownKeys$2(e7, r5) {
    var t5 = Object.keys(e7);
    if (Object.getOwnPropertySymbols) {
      var o7 = Object.getOwnPropertySymbols(e7);
      r5 && (o7 = o7.filter(function(r6) {
        return Object.getOwnPropertyDescriptor(e7, r6).enumerable;
      })), t5.push.apply(t5, o7);
    }
    return t5;
  }
  function _objectSpread$2(e7) {
    for (var r5 = 1; r5 < arguments.length; r5++) {
      var t5 = null != arguments[r5] ? arguments[r5] : {};
      r5 % 2 ? ownKeys$2(Object(t5), true).forEach(function(r6) {
        _defineProperty(e7, r6, t5[r6]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e7, Object.getOwnPropertyDescriptors(t5)) : ownKeys$2(Object(t5)).forEach(function(r6) {
        Object.defineProperty(e7, r6, Object.getOwnPropertyDescriptor(t5, r6));
      });
    }
    return e7;
  }
  function restrictor(plugin, params) {
    var scaling = params !== null && params !== void 0 && params.scaling ? params.scaling === true ? {
      min: 0.1,
      max: 1
    } : params.scaling : false;
    var translation = params !== null && params !== void 0 && params.translation ? params.translation === true ? {
      left: 0,
      top: 0,
      right: 1e3,
      bottom: 1e3
    } : params.translation : false;
    function restrictZoom(zoom) {
      if (!scaling) throw new Error("scaling param isnt defined");
      var _ref = typeof scaling === "function" ? scaling() : scaling, min3 = _ref.min, max3 = _ref.max;
      if (zoom < min3) {
        return min3;
      } else if (zoom > max3) {
        return max3;
      }
      return zoom;
    }
    function restrictPosition(position) {
      if (!translation) throw new Error("translation param isnt defined");
      var nextPosition = _objectSpread$2({}, position);
      var _ref2 = typeof translation === "function" ? translation() : translation, left = _ref2.left, top = _ref2.top, right = _ref2.right, bottom = _ref2.bottom;
      if (nextPosition.x < left) {
        nextPosition.x = left;
      }
      if (nextPosition.x > right) {
        nextPosition.x = right;
      }
      if (nextPosition.y < top) {
        nextPosition.y = top;
      }
      if (nextPosition.y > bottom) {
        nextPosition.y = bottom;
      }
      return nextPosition;
    }
    plugin.addPipe(function(context2) {
      if (!context2 || _typeof(context2) !== "object" || !("type" in context2)) return context2;
      if (scaling && context2.type === "zoom") {
        return _objectSpread$2(_objectSpread$2({}, context2), {}, {
          data: _objectSpread$2(_objectSpread$2({}, context2.data), {}, {
            zoom: restrictZoom(context2.data.zoom)
          })
        });
      }
      if (translation && context2.type === "zoomed") {
        var position = restrictPosition(plugin.area.transform);
        void plugin.area.translate(position.x, position.y);
      }
      if (translation && context2.type === "translate") {
        return _objectSpread$2(_objectSpread$2({}, context2), {}, {
          data: _objectSpread$2(_objectSpread$2({}, context2.data), {}, {
            position: restrictPosition(context2.data.position)
          })
        });
      }
      return context2;
    });
  }
  function accumulateOnCtrl() {
    var pressed = false;
    function keydown(e7) {
      if (e7.key === "Control" || e7.key === "Meta") pressed = true;
    }
    function keyup(e7) {
      if (e7.key === "Control" || e7.key === "Meta") pressed = false;
    }
    document.addEventListener("keydown", keydown);
    document.addEventListener("keyup", keyup);
    return {
      active: function active() {
        return pressed;
      },
      destroy: function destroy2() {
        document.removeEventListener("keydown", keydown);
        document.removeEventListener("keyup", keyup);
      }
    };
  }
  var Selector = /* @__PURE__ */ (function() {
    function Selector2() {
      _classCallCheck(this, Selector2);
      _defineProperty(this, "entities", /* @__PURE__ */ new Map());
      _defineProperty(this, "pickId", null);
    }
    return _createClass(Selector2, [{
      key: "isSelected",
      value: function isSelected(entity) {
        return this.entities.has("".concat(entity.label, "_").concat(entity.id));
      }
    }, {
      key: "add",
      value: (function() {
        var _add = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee(entity, accumulate) {
          var id;
          return import_regenerator2.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                id = "".concat(entity.label, "_").concat(entity.id);
                if (accumulate) {
                  _context.next = 4;
                  break;
                }
                _context.next = 4;
                return this.unselect(Array.from(this.entities.values()).filter(function(item) {
                  return item.label !== entity.label || item.id !== entity.id;
                }));
              case 4:
                this.entities.set(id, entity);
              case 5:
              case "end":
                return _context.stop();
            }
          }, _callee, this);
        }));
        function add(_x, _x2) {
          return _add.apply(this, arguments);
        }
        return add;
      })()
    }, {
      key: "remove",
      value: (function() {
        var _remove = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee2(entity) {
          var id, item;
          return import_regenerator2.default.wrap(function _callee2$(_context2) {
            while (1) switch (_context2.prev = _context2.next) {
              case 0:
                id = "".concat(entity.label, "_").concat(entity.id);
                item = this.entities.get(id);
                if (!item) {
                  _context2.next = 6;
                  break;
                }
                this.entities["delete"](id);
                _context2.next = 6;
                return item.unselect();
              case 6:
              case "end":
                return _context2.stop();
            }
          }, _callee2, this);
        }));
        function remove(_x3) {
          return _remove.apply(this, arguments);
        }
        return remove;
      })()
    }, {
      key: "unselect",
      value: (function() {
        var _unselect = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee3(entities) {
          var _this = this;
          return import_regenerator2.default.wrap(function _callee3$(_context3) {
            while (1) switch (_context3.prev = _context3.next) {
              case 0:
                _context3.next = 2;
                return Promise.all(Array.from(entities).map(function(entity) {
                  return _this.remove(entity);
                }));
              case 2:
              case "end":
                return _context3.stop();
            }
          }, _callee3);
        }));
        function unselect(_x4) {
          return _unselect.apply(this, arguments);
        }
        return unselect;
      })()
    }, {
      key: "unselectAll",
      value: (function() {
        var _unselectAll = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee4() {
          return import_regenerator2.default.wrap(function _callee4$(_context4) {
            while (1) switch (_context4.prev = _context4.next) {
              case 0:
                _context4.next = 2;
                return this.unselect(this.entities.values());
              case 2:
              case "end":
                return _context4.stop();
            }
          }, _callee4, this);
        }));
        function unselectAll() {
          return _unselectAll.apply(this, arguments);
        }
        return unselectAll;
      })()
    }, {
      key: "translate",
      value: (function() {
        var _translate = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee5(dx, dy) {
          var _this2 = this;
          return import_regenerator2.default.wrap(function _callee5$(_context5) {
            while (1) switch (_context5.prev = _context5.next) {
              case 0:
                _context5.next = 2;
                return Promise.all(Array.from(this.entities.values()).map(function(item) {
                  return !_this2.isPicked(item) && item.translate(dx, dy);
                }));
              case 2:
              case "end":
                return _context5.stop();
            }
          }, _callee5, this);
        }));
        function translate(_x5, _x6) {
          return _translate.apply(this, arguments);
        }
        return translate;
      })()
    }, {
      key: "pick",
      value: function pick(entity) {
        this.pickId = "".concat(entity.label, "_").concat(entity.id);
      }
    }, {
      key: "release",
      value: function release() {
        this.pickId = null;
      }
    }, {
      key: "isPicked",
      value: function isPicked(entity) {
        return this.pickId === "".concat(entity.label, "_").concat(entity.id);
      }
    }]);
  })();
  function selector() {
    return new Selector();
  }
  function selectableNodes(base, core, options) {
    var editor = null;
    var area = base;
    var getEditor = function getEditor2() {
      return editor || (editor = area.parentScope(NodeEditor));
    };
    var twitch = 0;
    function selectNode(node) {
      if (!node.selected) {
        node.selected = true;
        void area.update("node", node.id);
      }
    }
    function unselectNode(node) {
      if (node.selected) {
        node.selected = false;
        void area.update("node", node.id);
      }
    }
    function add(_x7, _x8) {
      return _add2.apply(this, arguments);
    }
    function _add2() {
      _add2 = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee8(nodeId, accumulate) {
        var node;
        return import_regenerator2.default.wrap(function _callee8$(_context8) {
          while (1) switch (_context8.prev = _context8.next) {
            case 0:
              node = getEditor().getNode(nodeId);
              if (node) {
                _context8.next = 3;
                break;
              }
              return _context8.abrupt("return");
            case 3:
              _context8.next = 5;
              return core.add({
                label: "node",
                id: node.id,
                translate: function translate(dx, dy) {
                  return _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee7() {
                    var view, current;
                    return import_regenerator2.default.wrap(function _callee7$(_context7) {
                      while (1) switch (_context7.prev = _context7.next) {
                        case 0:
                          view = area.nodeViews.get(node.id);
                          current = view === null || view === void 0 ? void 0 : view.position;
                          if (!current) {
                            _context7.next = 5;
                            break;
                          }
                          _context7.next = 5;
                          return view.translate(current.x + dx, current.y + dy);
                        case 5:
                        case "end":
                          return _context7.stop();
                      }
                    }, _callee7);
                  }))();
                },
                unselect: function unselect() {
                  unselectNode(node);
                }
              }, accumulate);
            case 5:
              selectNode(node);
            case 6:
            case "end":
              return _context8.stop();
          }
        }, _callee8);
      }));
      return _add2.apply(this, arguments);
    }
    function remove(_x9) {
      return _remove2.apply(this, arguments);
    }
    function _remove2() {
      _remove2 = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee9(nodeId) {
        return import_regenerator2.default.wrap(function _callee9$(_context9) {
          while (1) switch (_context9.prev = _context9.next) {
            case 0:
              _context9.next = 2;
              return core.remove({
                id: nodeId,
                label: "node"
              });
            case 2:
            case "end":
              return _context9.stop();
          }
        }, _callee9);
      }));
      return _remove2.apply(this, arguments);
    }
    area.addPipe(/* @__PURE__ */ (function() {
      var _ref = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee6(context2) {
        var pickedId, accumulate, _context$data, id, position, previous, _dx, _dy;
        return import_regenerator2.default.wrap(function _callee6$(_context6) {
          while (1) switch (_context6.prev = _context6.next) {
            case 0:
              if (!(!context2 || _typeof(context2) !== "object" || !("type" in context2))) {
                _context6.next = 2;
                break;
              }
              return _context6.abrupt("return", context2);
            case 2:
              if (!(context2.type === "nodepicked")) {
                _context6.next = 11;
                break;
              }
              pickedId = context2.data.id;
              accumulate = options.accumulating.active();
              core.pick({
                id: pickedId,
                label: "node"
              });
              twitch = null;
              _context6.next = 9;
              return add(pickedId, accumulate);
            case 9:
              _context6.next = 33;
              break;
            case 11:
              if (!(context2.type === "nodetranslated")) {
                _context6.next = 20;
                break;
              }
              _context$data = context2.data, id = _context$data.id, position = _context$data.position, previous = _context$data.previous;
              _dx = position.x - previous.x;
              _dy = position.y - previous.y;
              if (!core.isPicked({
                id,
                label: "node"
              })) {
                _context6.next = 18;
                break;
              }
              _context6.next = 18;
              return core.translate(_dx, _dy);
            case 18:
              _context6.next = 33;
              break;
            case 20:
              if (!(context2.type === "pointerdown")) {
                _context6.next = 24;
                break;
              }
              twitch = 0;
              _context6.next = 33;
              break;
            case 24:
              if (!(context2.type === "pointermove")) {
                _context6.next = 28;
                break;
              }
              if (twitch !== null) twitch++;
              _context6.next = 33;
              break;
            case 28:
              if (!(context2.type === "pointerup")) {
                _context6.next = 33;
                break;
              }
              if (!(twitch !== null && twitch < 4)) {
                _context6.next = 32;
                break;
              }
              _context6.next = 32;
              return core.unselectAll();
            case 32:
              twitch = null;
            case 33:
              return _context6.abrupt("return", context2);
            case 34:
            case "end":
              return _context6.stop();
          }
        }, _callee6);
      }));
      return function(_x10) {
        return _ref.apply(this, arguments);
      };
    })());
    return {
      select: add,
      unselect: remove
    };
  }
  function showInputControl(area, visible) {
    var editor = null;
    var getEditor = function getEditor2() {
      return editor || (editor = area.parentScope(NodeEditor));
    };
    function updateInputControlVisibility(target, targetInput) {
      var node = getEditor().getNode(target);
      if (!node) return;
      var input = node.inputs[targetInput];
      if (!input) throw new Error("cannot find input");
      var previous = input.showControl;
      var connections = getEditor().getConnections();
      var hasAnyConnection = Boolean(connections.find(function(connection) {
        return connection.target === target && connection.targetInput === targetInput;
      }));
      input.showControl = visible ? visible({
        hasAnyConnection,
        input
      }) : !hasAnyConnection;
      if (input.showControl !== previous) {
        void area.update("node", node.id);
      }
    }
    area.addPipe(function(context2) {
      if (context2.type === "connectioncreated" || context2.type === "connectionremoved") {
        updateInputControlVisibility(context2.data.target, context2.data.targetInput);
      }
      return context2;
    });
  }
  function ownKeys$1(e7, r5) {
    var t5 = Object.keys(e7);
    if (Object.getOwnPropertySymbols) {
      var o7 = Object.getOwnPropertySymbols(e7);
      r5 && (o7 = o7.filter(function(r6) {
        return Object.getOwnPropertyDescriptor(e7, r6).enumerable;
      })), t5.push.apply(t5, o7);
    }
    return t5;
  }
  function _objectSpread$1(e7) {
    for (var r5 = 1; r5 < arguments.length; r5++) {
      var t5 = null != arguments[r5] ? arguments[r5] : {};
      r5 % 2 ? ownKeys$1(Object(t5), true).forEach(function(r6) {
        _defineProperty(e7, r6, t5[r6]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e7, Object.getOwnPropertyDescriptors(t5)) : ownKeys$1(Object(t5)).forEach(function(r6) {
        Object.defineProperty(e7, r6, Object.getOwnPropertyDescriptor(t5, r6));
      });
    }
    return e7;
  }
  function snapGrid(base, params) {
    var area = base;
    var size = typeof (params === null || params === void 0 ? void 0 : params.size) === "undefined" ? 16 : params.size;
    var dynamic = typeof (params === null || params === void 0 ? void 0 : params.dynamic) === "undefined" ? true : params.dynamic;
    function snap(value) {
      return Math.round(value / size) * size;
    }
    area.addPipe(function(context2) {
      if (!context2 || _typeof(context2) !== "object" || !("type" in context2)) return context2;
      if (dynamic && context2.type === "nodetranslate") {
        var position = context2.data.position;
        var x2 = snap(position.x);
        var y3 = snap(position.y);
        return _objectSpread$1(_objectSpread$1({}, context2), {}, {
          data: _objectSpread$1(_objectSpread$1({}, context2.data), {}, {
            position: {
              x: x2,
              y: y3
            }
          })
        });
      }
      if (!dynamic && context2.type === "nodedragged") {
        var view = area.nodeViews.get(context2.data.id);
        if (view) {
          var _view$position = view.position, _x = _view$position.x, _y = _view$position.y;
          void view.translate(snap(_x), snap(_y));
        }
      }
      return context2;
    });
  }
  function zoomAt(_x, _x2, _x3) {
    return _zoomAt.apply(this, arguments);
  }
  function _zoomAt() {
    _zoomAt = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee(plugin, nodes, params) {
      var _ref, _ref$scale, scale, editor, list, rects, boundingBox, _ref2, w2, h3, kw, kh, k2;
      return import_regenerator2.default.wrap(function _callee$(_context) {
        while (1) switch (_context.prev = _context.next) {
          case 0:
            _ref = params || {}, _ref$scale = _ref.scale, scale = _ref$scale === void 0 ? 0.9 : _ref$scale;
            editor = plugin.parentScope(NodeEditor);
            list = nodes.map(function(node) {
              return _typeof(node) === "object" ? node : editor.getNode(node);
            });
            rects = getNodesRect(list, plugin.nodeViews);
            boundingBox = getBoundingBox$1(rects);
            _ref2 = [plugin.container.clientWidth, plugin.container.clientHeight], w2 = _ref2[0], h3 = _ref2[1];
            kw = w2 / boundingBox.width, kh = h3 / boundingBox.height;
            k2 = Math.min(kh * scale, kw * scale, 1);
            plugin.area.transform.x = w2 / 2 - boundingBox.center.x * k2;
            plugin.area.transform.y = h3 / 2 - boundingBox.center.y * k2;
            _context.next = 12;
            return plugin.area.zoom(k2, 0, 0);
          case 12:
          case "end":
            return _context.stop();
        }
      }, _callee);
    }));
    return _zoomAt.apply(this, arguments);
  }
  var index = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    getBoundingBox,
    simpleNodesOrder,
    zIndexNodesOrder,
    restrictor,
    accumulateOnCtrl,
    selectableNodes,
    Selector,
    selector,
    showInputControl,
    snapGrid,
    zoomAt
  });
  function ownKeys(e7, r5) {
    var t5 = Object.keys(e7);
    if (Object.getOwnPropertySymbols) {
      var o7 = Object.getOwnPropertySymbols(e7);
      r5 && (o7 = o7.filter(function(r6) {
        return Object.getOwnPropertyDescriptor(e7, r6).enumerable;
      })), t5.push.apply(t5, o7);
    }
    return t5;
  }
  function _objectSpread(e7) {
    for (var r5 = 1; r5 < arguments.length; r5++) {
      var t5 = null != arguments[r5] ? arguments[r5] : {};
      r5 % 2 ? ownKeys(Object(t5), true).forEach(function(r6) {
        _defineProperty(e7, r6, t5[r6]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e7, Object.getOwnPropertyDescriptors(t5)) : ownKeys(Object(t5)).forEach(function(r6) {
        Object.defineProperty(e7, r6, Object.getOwnPropertyDescriptor(t5, r6));
      });
    }
    return e7;
  }
  function _callSuper2(t5, o7, e7) {
    return o7 = _getPrototypeOf(o7), _possibleConstructorReturn(t5, _isNativeReflectConstruct2() ? Reflect.construct(o7, e7 || [], _getPrototypeOf(t5).constructor) : o7.apply(t5, e7));
  }
  function _isNativeReflectConstruct2() {
    try {
      var t5 = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
    } catch (t6) {
    }
    return (_isNativeReflectConstruct2 = function _isNativeReflectConstruct6() {
      return !!t5;
    })();
  }
  var AreaPlugin = /* @__PURE__ */ (function(_BaseAreaPlugin) {
    function AreaPlugin2(container) {
      var _this;
      _classCallCheck(this, AreaPlugin2);
      _this = _callSuper2(this, AreaPlugin2, ["area"]);
      _defineProperty(_this, "nodeViews", /* @__PURE__ */ new Map());
      _defineProperty(_this, "connectionViews", /* @__PURE__ */ new Map());
      _defineProperty(_this, "elements", new ElementsHolder());
      _defineProperty(_this, "onContextMenu", function(event) {
        void _this.emit({
          type: "contextmenu",
          data: {
            event,
            context: "root"
          }
        });
      });
      _this.container = container;
      container.style.overflow = "hidden";
      container.addEventListener("contextmenu", _this.onContextMenu);
      _this.addPipe(function(context2) {
        if (!context2 || !(_typeof(context2) === "object" && "type" in context2)) return context2;
        if (context2.type === "nodecreated") {
          _this.addNodeView(context2.data);
        }
        if (context2.type === "noderemoved") {
          _this.removeNodeView(context2.data.id);
        }
        if (context2.type === "connectioncreated") {
          _this.addConnectionView(context2.data);
        }
        if (context2.type === "connectionremoved") {
          _this.removeConnectionView(context2.data.id);
        }
        if (context2.type === "render") {
          _this.elements.set(context2.data);
        }
        if (context2.type === "unmount") {
          _this.elements["delete"](context2.data.element);
        }
        return context2;
      });
      _this.area = new Area(container, {
        zoomed: function zoomed(params) {
          return _this.emit({
            type: "zoomed",
            data: params
          });
        },
        pointerDown: function pointerDown(position, event) {
          return void _this.emit({
            type: "pointerdown",
            data: {
              position,
              event
            }
          });
        },
        pointerMove: function pointerMove(position, event) {
          return void _this.emit({
            type: "pointermove",
            data: {
              position,
              event
            }
          });
        },
        pointerUp: function pointerUp(position, event) {
          return void _this.emit({
            type: "pointerup",
            data: {
              position,
              event
            }
          });
        },
        resize: function resize(event) {
          return void _this.emit({
            type: "resized",
            data: {
              event
            }
          });
        },
        translated: function translated(params) {
          return _this.emit({
            type: "translated",
            data: params
          });
        },
        reordered: function reordered(element) {
          return _this.emit({
            type: "reordered",
            data: {
              element
            }
          });
        }
      }, {
        translate: function translate(params) {
          return _this.emit({
            type: "translate",
            data: params
          });
        },
        zoom: function zoom(params) {
          return _this.emit({
            type: "zoom",
            data: params
          });
        }
      });
      return _this;
    }
    _inherits(AreaPlugin2, _BaseAreaPlugin);
    return _createClass(AreaPlugin2, [{
      key: "addNodeView",
      value: function addNodeView(node) {
        var _this2 = this;
        var id = node.id;
        var view = new NodeView(function() {
          return _this2.area.transform.k;
        }, {
          picked: function picked() {
            return void _this2.emit({
              type: "nodepicked",
              data: {
                id
              }
            });
          },
          translated: function translated(data) {
            return _this2.emit({
              type: "nodetranslated",
              data: _objectSpread({
                id
              }, data)
            });
          },
          dragged: function dragged() {
            return void _this2.emit({
              type: "nodedragged",
              data: node
            });
          },
          contextmenu: function contextmenu(event) {
            return void _this2.emit({
              type: "contextmenu",
              data: {
                event,
                context: node
              }
            });
          },
          resized: function resized(_ref) {
            var size = _ref.size;
            return _this2.emit({
              type: "noderesized",
              data: {
                id: node.id,
                size
              }
            });
          }
        }, {
          translate: function translate(data) {
            return _this2.emit({
              type: "nodetranslate",
              data: _objectSpread({
                id
              }, data)
            });
          },
          resize: function resize(_ref2) {
            var size = _ref2.size;
            return _this2.emit({
              type: "noderesize",
              data: {
                id: node.id,
                size
              }
            });
          }
        });
        this.nodeViews.set(id, view);
        this.area.content.add(view.element);
        void this.emit({
          type: "render",
          data: {
            element: view.element,
            type: "node",
            payload: node
          }
        });
        return view;
      }
    }, {
      key: "removeNodeView",
      value: function removeNodeView(id) {
        var view = this.nodeViews.get(id);
        if (view) {
          void this.emit({
            type: "unmount",
            data: {
              element: view.element
            }
          });
          this.nodeViews["delete"](id);
          this.area.content.remove(view.element);
        }
      }
    }, {
      key: "addConnectionView",
      value: function addConnectionView(connection) {
        var _this3 = this;
        var view = new ConnectionView({
          contextmenu: function contextmenu(event) {
            return void _this3.emit({
              type: "contextmenu",
              data: {
                event,
                context: connection
              }
            });
          }
        });
        this.connectionViews.set(connection.id, view);
        this.area.content.add(view.element);
        void this.emit({
          type: "render",
          data: {
            element: view.element,
            type: "connection",
            payload: connection
          }
        });
        return view;
      }
    }, {
      key: "removeConnectionView",
      value: function removeConnectionView(id) {
        var view = this.connectionViews.get(id);
        if (view) {
          void this.emit({
            type: "unmount",
            data: {
              element: view.element
            }
          });
          this.connectionViews["delete"](id);
          this.area.content.remove(view.element);
        }
      }
      /**
       * Force update rendered element by id (node, connection, etc.)
       * @param type Element type
       * @param id Element id
       * @emits render
       */
    }, {
      key: "update",
      value: (function() {
        var _update2 = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee(type, id) {
          var data;
          return import_regenerator2.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                data = this.elements.get(type, id);
                if (!data) {
                  _context.next = 4;
                  break;
                }
                _context.next = 4;
                return this.emit({
                  type: "render",
                  data
                });
              case 4:
              case "end":
                return _context.stop();
            }
          }, _callee, this);
        }));
        function update(_x, _x2) {
          return _update2.apply(this, arguments);
        }
        return update;
      })()
    }, {
      key: "resize",
      value: (function() {
        var _resize = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee2(id, width, height) {
          var view;
          return import_regenerator2.default.wrap(function _callee2$(_context2) {
            while (1) switch (_context2.prev = _context2.next) {
              case 0:
                view = this.nodeViews.get(id);
                if (!view) {
                  _context2.next = 5;
                  break;
                }
                _context2.next = 4;
                return view.resize(width, height);
              case 4:
                return _context2.abrupt("return", _context2.sent);
              case 5:
              case "end":
                return _context2.stop();
            }
          }, _callee2, this);
        }));
        function resize(_x3, _x4, _x5) {
          return _resize.apply(this, arguments);
        }
        return resize;
      })()
    }, {
      key: "translate",
      value: (function() {
        var _translate = _asyncToGenerator(/* @__PURE__ */ import_regenerator2.default.mark(function _callee3(id, _ref3) {
          var x2, y3, view;
          return import_regenerator2.default.wrap(function _callee3$(_context3) {
            while (1) switch (_context3.prev = _context3.next) {
              case 0:
                x2 = _ref3.x, y3 = _ref3.y;
                view = this.nodeViews.get(id);
                if (!view) {
                  _context3.next = 6;
                  break;
                }
                _context3.next = 5;
                return view.translate(x2, y3);
              case 5:
                return _context3.abrupt("return", _context3.sent);
              case 6:
              case "end":
                return _context3.stop();
            }
          }, _callee3, this);
        }));
        function translate(_x6, _x7) {
          return _translate.apply(this, arguments);
        }
        return translate;
      })()
    }, {
      key: "destroy",
      value: function destroy2() {
        var _this4 = this;
        this.container.removeEventListener("contextmenu", this.onContextMenu);
        Array.from(this.connectionViews.keys()).forEach(function(id) {
          _this4.removeConnectionView(id);
        });
        Array.from(this.nodeViews.keys()).forEach(function(id) {
          _this4.removeNodeView(id);
        });
        this.area.destroy();
      }
    }]);
  })(BaseAreaPlugin);

  // node_modules/@babel/runtime/helpers/esm/superPropBase.js
  function _superPropBase(t5, o7) {
    for (; !{}.hasOwnProperty.call(t5, o7) && null !== (t5 = _getPrototypeOf(t5)); ) ;
    return t5;
  }

  // node_modules/@babel/runtime/helpers/esm/get.js
  function _get() {
    return _get = "undefined" != typeof Reflect && Reflect.get ? Reflect.get.bind() : function(e7, t5, r5) {
      var p3 = _superPropBase(e7, t5);
      if (p3) {
        var n6 = Object.getOwnPropertyDescriptor(p3, t5);
        return n6.get ? n6.get.call(arguments.length < 3 ? e7 : r5) : n6.value;
      }
    }, _get.apply(null, arguments);
  }

  // node_modules/rete-connection-plugin/rete-connection-plugin.esm.js
  var import_regenerator3 = __toESM(require_regenerator2());

  // node_modules/@babel/runtime/helpers/esm/arrayWithHoles.js
  function _arrayWithHoles(r5) {
    if (Array.isArray(r5)) return r5;
  }

  // node_modules/@babel/runtime/helpers/esm/iterableToArrayLimit.js
  function _iterableToArrayLimit(r5, l3) {
    var t5 = null == r5 ? null : "undefined" != typeof Symbol && r5[Symbol.iterator] || r5["@@iterator"];
    if (null != t5) {
      var e7, n6, i7, u3, a3 = [], f3 = true, o7 = false;
      try {
        if (i7 = (t5 = t5.call(r5)).next, 0 === l3) {
          if (Object(t5) !== t5) return;
          f3 = false;
        } else for (; !(f3 = (e7 = i7.call(t5)).done) && (a3.push(e7.value), a3.length !== l3); f3 = true) ;
      } catch (r6) {
        o7 = true, n6 = r6;
      } finally {
        try {
          if (!f3 && null != t5["return"] && (u3 = t5["return"](), Object(u3) !== u3)) return;
        } finally {
          if (o7) throw n6;
        }
      }
      return a3;
    }
  }

  // node_modules/@babel/runtime/helpers/esm/nonIterableRest.js
  function _nonIterableRest() {
    throw new TypeError("Invalid attempt to destructure non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
  }

  // node_modules/@babel/runtime/helpers/esm/slicedToArray.js
  function _slicedToArray(r5, e7) {
    return _arrayWithHoles(r5) || _iterableToArrayLimit(r5, e7) || _unsupportedIterableToArray2(r5, e7) || _nonIterableRest();
  }

  // node_modules/rete-connection-plugin/rete-connection-plugin.esm.js
  function ownKeys2(e7, r5) {
    var t5 = Object.keys(e7);
    if (Object.getOwnPropertySymbols) {
      var o7 = Object.getOwnPropertySymbols(e7);
      r5 && (o7 = o7.filter(function(r6) {
        return Object.getOwnPropertyDescriptor(e7, r6).enumerable;
      })), t5.push.apply(t5, o7);
    }
    return t5;
  }
  function _objectSpread2(e7) {
    for (var r5 = 1; r5 < arguments.length; r5++) {
      var t5 = null != arguments[r5] ? arguments[r5] : {};
      r5 % 2 ? ownKeys2(Object(t5), true).forEach(function(r6) {
        _defineProperty(e7, r6, t5[r6]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e7, Object.getOwnPropertyDescriptors(t5)) : ownKeys2(Object(t5)).forEach(function(r6) {
        Object.defineProperty(e7, r6, Object.getOwnPropertyDescriptor(t5, r6));
      });
    }
    return e7;
  }
  function createPseudoconnection(extra) {
    var element = null;
    var id = null;
    function unmount(areaPlugin) {
      if (id) {
        areaPlugin.removeConnectionView(id);
      }
      element = null;
      id = null;
    }
    function mount(areaPlugin) {
      unmount(areaPlugin);
      id = "pseudo_".concat(getUID());
    }
    return {
      isMounted: function isMounted() {
        return Boolean(id);
      },
      mount,
      render: function render(areaPlugin, _ref, data) {
        var x2 = _ref.x, y3 = _ref.y;
        var isOutput = data.side === "output";
        var pointer = {
          x: x2 + (isOutput ? -3 : 3),
          y: y3
        };
        if (!id) throw new Error("pseudo connection id wasn't generated");
        var payload = isOutput ? _objectSpread2({
          id,
          source: data.nodeId,
          sourceOutput: data.key,
          target: "",
          targetInput: ""
        }, extra !== null && extra !== void 0 ? extra : {}) : _objectSpread2({
          id,
          target: data.nodeId,
          targetInput: data.key,
          source: "",
          sourceOutput: ""
        }, extra !== null && extra !== void 0 ? extra : {});
        if (!element) {
          var view = areaPlugin.addConnectionView(payload);
          element = view.element;
        }
        if (!element) return;
        void areaPlugin.emit({
          type: "render",
          data: _objectSpread2({
            element,
            type: "connection",
            payload
          }, isOutput ? {
            end: pointer
          } : {
            start: pointer
          })
        });
      },
      unmount
    };
  }
  function _createForOfIteratorHelper$12(r5, e7) {
    var t5 = "undefined" != typeof Symbol && r5[Symbol.iterator] || r5["@@iterator"];
    if (!t5) {
      if (Array.isArray(r5) || (t5 = _unsupportedIterableToArray$12(r5)) || e7 && r5 && "number" == typeof r5.length) {
        t5 && (r5 = t5);
        var _n = 0, F = function F2() {
        };
        return { s: F, n: function n6() {
          return _n >= r5.length ? { done: true } : { done: false, value: r5[_n++] };
        }, e: function e8(r6) {
          throw r6;
        }, f: F };
      }
      throw new TypeError("Invalid attempt to iterate non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
    }
    var o7, a3 = true, u3 = false;
    return { s: function s4() {
      t5 = t5.call(r5);
    }, n: function n6() {
      var r6 = t5.next();
      return a3 = r6.done, r6;
    }, e: function e8(r6) {
      u3 = true, o7 = r6;
    }, f: function f3() {
      try {
        a3 || null == t5["return"] || t5["return"]();
      } finally {
        if (u3) throw o7;
      }
    } };
  }
  function _unsupportedIterableToArray$12(r5, a3) {
    if (r5) {
      if ("string" == typeof r5) return _arrayLikeToArray$12(r5, a3);
      var t5 = {}.toString.call(r5).slice(8, -1);
      return "Object" === t5 && r5.constructor && (t5 = r5.constructor.name), "Map" === t5 || "Set" === t5 ? Array.from(r5) : "Arguments" === t5 || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(t5) ? _arrayLikeToArray$12(r5, a3) : void 0;
    }
  }
  function _arrayLikeToArray$12(r5, a3) {
    (null == a3 || a3 > r5.length) && (a3 = r5.length);
    for (var e7 = 0, n6 = Array(a3); e7 < a3; e7++) n6[e7] = r5[e7];
    return n6;
  }
  function findSocket(socketsCache, elements) {
    var _iterator = _createForOfIteratorHelper$12(elements), _step;
    try {
      for (_iterator.s(); !(_step = _iterator.n()).done; ) {
        var element = _step.value;
        var found = socketsCache.get(element);
        if (found) {
          return found;
        }
      }
    } catch (err) {
      _iterator.e(err);
    } finally {
      _iterator.f();
    }
  }
  function elementsFromPoint(x2, y3) {
    var _elements$;
    var root = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : document;
    var elements = root.elementsFromPoint(x2, y3);
    var shadowRoot = (_elements$ = elements[0]) === null || _elements$ === void 0 ? void 0 : _elements$.shadowRoot;
    if (shadowRoot && shadowRoot !== root) {
      elements.unshift.apply(elements, _toConsumableArray(elementsFromPoint(x2, y3, shadowRoot)));
    }
    return elements;
  }
  var State = /* @__PURE__ */ (function() {
    function State2() {
      _classCallCheck(this, State2);
    }
    return _createClass(State2, [{
      key: "setContext",
      value: function setContext(context2) {
        this.context = context2;
      }
    }]);
  })();
  function getSourceTarget(initial, socket) {
    var forward = initial.side === "output" && socket.side === "input";
    var backward = initial.side === "input" && socket.side === "output";
    var _ref = forward ? [initial, socket] : backward ? [socket, initial] : [], _ref2 = _slicedToArray(_ref, 2), source = _ref2[0], target = _ref2[1];
    if (source && target) return [source, target];
  }
  function canMakeConnection(initial, socket) {
    return Boolean(getSourceTarget(initial, socket));
  }
  function makeConnection(initial, socket, context2) {
    var _ref3 = getSourceTarget(initial, socket) || [null, null], _ref4 = _slicedToArray(_ref3, 2), source = _ref4[0], target = _ref4[1];
    if (source && target) {
      void context2.editor.addConnection({
        id: getUID(),
        source: source.nodeId,
        sourceOutput: source.key,
        target: target.nodeId,
        targetInput: target.key
      });
      return true;
    }
  }
  function findPort(socket, editor) {
    var node = editor.getNode(socket.nodeId);
    if (!node) throw new Error("cannot find node");
    var list = socket.side === "input" ? node.inputs : node.outputs;
    return list[socket.key];
  }
  function findConnections(socket, editor) {
    var nodeId = socket.nodeId, side = socket.side, key = socket.key;
    return editor.getConnections().filter(function(connection) {
      if (side === "input") {
        return connection.target === nodeId && connection.targetInput === key;
      }
      if (side === "output") {
        return connection.source === nodeId && connection.sourceOutput === key;
      }
    });
  }
  function syncConnections(sockets, editor) {
    var connections = sockets.map(function(socket) {
      var port = findPort(socket, editor);
      var multiple = port === null || port === void 0 ? void 0 : port.multipleConnections;
      if (multiple) return [];
      return findConnections(socket, editor);
    }).flat();
    return {
      commit: function commit() {
        var uniqueIds = Array.from(new Set(connections.map(function(_ref) {
          var id = _ref.id;
          return id;
        })));
        uniqueIds.forEach(function(id) {
          return void editor.removeConnection(id);
        });
      }
    };
  }
  function _callSuper$13(t5, o7, e7) {
    return o7 = _getPrototypeOf(o7), _possibleConstructorReturn(t5, _isNativeReflectConstruct$13() ? Reflect.construct(o7, e7 || [], _getPrototypeOf(t5).constructor) : o7.apply(t5, e7));
  }
  function _isNativeReflectConstruct$13() {
    try {
      var t5 = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
    } catch (t6) {
    }
    return (_isNativeReflectConstruct$13 = function _isNativeReflectConstruct6() {
      return !!t5;
    })();
  }
  var Picked = /* @__PURE__ */ (function(_State) {
    function Picked2(initial, params) {
      var _this;
      _classCallCheck(this, Picked2);
      _this = _callSuper$13(this, Picked2);
      _this.initial = initial;
      _this.params = params;
      return _this;
    }
    _inherits(Picked2, _State);
    return _createClass(Picked2, [{
      key: "pick",
      value: (function() {
        var _pick = _asyncToGenerator(/* @__PURE__ */ import_regenerator3.default.mark(function _callee(_ref, context2) {
          var socket, created;
          return import_regenerator3.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                socket = _ref.socket;
                if (this.params.canMakeConnection(this.initial, socket)) {
                  syncConnections([this.initial, socket], context2.editor).commit();
                  created = this.params.makeConnection(this.initial, socket, context2);
                  this.drop(context2, created ? socket : null, created);
                }
              case 2:
              case "end":
                return _context.stop();
            }
          }, _callee, this);
        }));
        function pick(_x, _x2) {
          return _pick.apply(this, arguments);
        }
        return pick;
      })()
    }, {
      key: "drop",
      value: function drop(context2) {
        var socket = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : null;
        var created = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : false;
        if (this.initial) {
          void context2.scope.emit({
            type: "connectiondrop",
            data: {
              initial: this.initial,
              socket,
              created
            }
          });
        }
        this.context.switchTo(new Idle(this.params));
      }
    }]);
  })(State);
  var PickedExisting = /* @__PURE__ */ (function(_State2) {
    function PickedExisting2(connection, params, context2) {
      var _this2;
      _classCallCheck(this, PickedExisting2);
      _this2 = _callSuper$13(this, PickedExisting2);
      _this2.connection = connection;
      _this2.params = params;
      var outputSocket = Array.from(context2.socketsCache.values()).find(function(data) {
        return data.nodeId === _this2.connection.source && data.side === "output" && data.key === _this2.connection.sourceOutput;
      });
      if (!outputSocket) throw new Error("cannot find output socket");
      _this2.outputSocket = outputSocket;
      return _this2;
    }
    _inherits(PickedExisting2, _State2);
    return _createClass(PickedExisting2, [{
      key: "init",
      value: (function() {
        var _init = _asyncToGenerator(/* @__PURE__ */ import_regenerator3.default.mark(function _callee2(context2) {
          var _this3 = this;
          return import_regenerator3.default.wrap(function _callee2$(_context2) {
            while (1) switch (_context2.prev = _context2.next) {
              case 0:
                void context2.scope.emit({
                  type: "connectionpick",
                  data: {
                    socket: this.outputSocket
                  }
                }).then(function(response) {
                  if (response) {
                    void context2.editor.removeConnection(_this3.connection.id);
                    _this3.initial = _this3.outputSocket;
                  } else {
                    _this3.drop(context2);
                  }
                });
              case 1:
              case "end":
                return _context2.stop();
            }
          }, _callee2, this);
        }));
        function init(_x3) {
          return _init.apply(this, arguments);
        }
        return init;
      })()
    }, {
      key: "pick",
      value: (function() {
        var _pick2 = _asyncToGenerator(/* @__PURE__ */ import_regenerator3.default.mark(function _callee3(_ref2, context2) {
          var socket, event, created, droppedSocket, _created, _droppedSocket;
          return import_regenerator3.default.wrap(function _callee3$(_context3) {
            while (1) switch (_context3.prev = _context3.next) {
              case 0:
                socket = _ref2.socket, event = _ref2.event;
                if (this.initial && !(socket.side === "input" && this.connection.target === socket.nodeId && this.connection.targetInput === socket.key)) {
                  if (this.params.canMakeConnection(this.initial, socket)) {
                    syncConnections([this.initial, socket], context2.editor).commit();
                    created = this.params.makeConnection(this.initial, socket, context2);
                    droppedSocket = created ? socket : null;
                    this.drop(context2, droppedSocket, created);
                  }
                } else if (event === "down") {
                  if (this.initial) {
                    syncConnections([this.initial, socket], context2.editor).commit();
                    _created = this.params.makeConnection(this.initial, socket, context2);
                    _droppedSocket = _created ? null : socket;
                    this.drop(context2, _droppedSocket, _created);
                  }
                }
              case 2:
              case "end":
                return _context3.stop();
            }
          }, _callee3, this);
        }));
        function pick(_x4, _x5) {
          return _pick2.apply(this, arguments);
        }
        return pick;
      })()
    }, {
      key: "drop",
      value: function drop(context2) {
        var socket = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : null;
        var created = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : false;
        if (this.initial) {
          void context2.scope.emit({
            type: "connectiondrop",
            data: {
              initial: this.initial,
              socket,
              created
            }
          });
        }
        this.context.switchTo(new Idle(this.params));
      }
    }]);
  })(State);
  var Idle = /* @__PURE__ */ (function(_State3) {
    function Idle2(params) {
      var _this4;
      _classCallCheck(this, Idle2);
      _this4 = _callSuper$13(this, Idle2);
      _this4.params = params;
      return _this4;
    }
    _inherits(Idle2, _State3);
    return _createClass(Idle2, [{
      key: "pick",
      value: (function() {
        var _pick3 = _asyncToGenerator(/* @__PURE__ */ import_regenerator3.default.mark(function _callee4(_ref3, context2) {
          var socket, event, _connection, state;
          return import_regenerator3.default.wrap(function _callee4$(_context4) {
            while (1) switch (_context4.prev = _context4.next) {
              case 0:
                socket = _ref3.socket, event = _ref3.event;
                if (!(event !== "down")) {
                  _context4.next = 3;
                  break;
                }
                return _context4.abrupt("return");
              case 3:
                if (!(socket.side === "input")) {
                  _context4.next = 11;
                  break;
                }
                _connection = context2.editor.getConnections().find(function(item) {
                  return item.target === socket.nodeId && item.targetInput === socket.key;
                });
                if (!_connection) {
                  _context4.next = 11;
                  break;
                }
                state = new PickedExisting(_connection, this.params, context2);
                _context4.next = 9;
                return state.init(context2);
              case 9:
                this.context.switchTo(state);
                return _context4.abrupt("return");
              case 11:
                _context4.next = 13;
                return context2.scope.emit({
                  type: "connectionpick",
                  data: {
                    socket
                  }
                });
              case 13:
                if (!_context4.sent) {
                  _context4.next = 17;
                  break;
                }
                this.context.switchTo(new Picked(socket, this.params));
                _context4.next = 18;
                break;
              case 17:
                this.drop(context2);
              case 18:
              case "end":
                return _context4.stop();
            }
          }, _callee4, this);
        }));
        function pick(_x6, _x7) {
          return _pick3.apply(this, arguments);
        }
        return pick;
      })()
    }, {
      key: "drop",
      value: function drop(context2) {
        var socket = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : null;
        var created = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : false;
        if (this.initial) {
          void context2.scope.emit({
            type: "connectiondrop",
            data: {
              initial: this.initial,
              socket,
              created
            }
          });
        }
        delete this.initial;
      }
    }]);
  })(State);
  var ClassicFlow = /* @__PURE__ */ (function() {
    function ClassicFlow2(params) {
      _classCallCheck(this, ClassicFlow2);
      var canMakeConnection$1 = (params === null || params === void 0 ? void 0 : params.canMakeConnection) || canMakeConnection;
      var makeConnection$1 = (params === null || params === void 0 ? void 0 : params.makeConnection) || makeConnection;
      this.switchTo(new Idle({
        canMakeConnection: canMakeConnection$1,
        makeConnection: makeConnection$1
      }));
    }
    return _createClass(ClassicFlow2, [{
      key: "pick",
      value: (function() {
        var _pick4 = _asyncToGenerator(/* @__PURE__ */ import_regenerator3.default.mark(function _callee5(params, context2) {
          return import_regenerator3.default.wrap(function _callee5$(_context5) {
            while (1) switch (_context5.prev = _context5.next) {
              case 0:
                _context5.next = 2;
                return this.currentState.pick(params, context2);
              case 2:
              case "end":
                return _context5.stop();
            }
          }, _callee5, this);
        }));
        function pick(_x8, _x9) {
          return _pick4.apply(this, arguments);
        }
        return pick;
      })()
    }, {
      key: "getPickedSocket",
      value: function getPickedSocket() {
        return this.currentState.initial;
      }
    }, {
      key: "switchTo",
      value: function switchTo(state) {
        state.setContext(this);
        this.currentState = state;
      }
    }, {
      key: "drop",
      value: function drop(context2) {
        this.currentState.drop(context2);
      }
    }]);
  })();
  function setup() {
    return function() {
      return new ClassicFlow();
    };
  }
  var classic2 = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    setup
  });
  var index2 = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    classic: classic2
  });
  function _createForOfIteratorHelper2(r5, e7) {
    var t5 = "undefined" != typeof Symbol && r5[Symbol.iterator] || r5["@@iterator"];
    if (!t5) {
      if (Array.isArray(r5) || (t5 = _unsupportedIterableToArray3(r5)) || e7 && r5 && "number" == typeof r5.length) {
        t5 && (r5 = t5);
        var _n = 0, F = function F2() {
        };
        return { s: F, n: function n6() {
          return _n >= r5.length ? { done: true } : { done: false, value: r5[_n++] };
        }, e: function e8(r6) {
          throw r6;
        }, f: F };
      }
      throw new TypeError("Invalid attempt to iterate non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
    }
    var o7, a3 = true, u3 = false;
    return { s: function s4() {
      t5 = t5.call(r5);
    }, n: function n6() {
      var r6 = t5.next();
      return a3 = r6.done, r6;
    }, e: function e8(r6) {
      u3 = true, o7 = r6;
    }, f: function f3() {
      try {
        a3 || null == t5["return"] || t5["return"]();
      } finally {
        if (u3) throw o7;
      }
    } };
  }
  function _unsupportedIterableToArray3(r5, a3) {
    if (r5) {
      if ("string" == typeof r5) return _arrayLikeToArray3(r5, a3);
      var t5 = {}.toString.call(r5).slice(8, -1);
      return "Object" === t5 && r5.constructor && (t5 = r5.constructor.name), "Map" === t5 || "Set" === t5 ? Array.from(r5) : "Arguments" === t5 || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(t5) ? _arrayLikeToArray3(r5, a3) : void 0;
    }
  }
  function _arrayLikeToArray3(r5, a3) {
    (null == a3 || a3 > r5.length) && (a3 = r5.length);
    for (var e7 = 0, n6 = Array(a3); e7 < a3; e7++) n6[e7] = r5[e7];
    return n6;
  }
  function _callSuper3(t5, o7, e7) {
    return o7 = _getPrototypeOf(o7), _possibleConstructorReturn(t5, _isNativeReflectConstruct3() ? Reflect.construct(o7, e7 || [], _getPrototypeOf(t5).constructor) : o7.apply(t5, e7));
  }
  function _isNativeReflectConstruct3() {
    try {
      var t5 = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
    } catch (t6) {
    }
    return (_isNativeReflectConstruct3 = function _isNativeReflectConstruct6() {
      return !!t5;
    })();
  }
  function _superPropGet(t5, e7, o7, r5) {
    var p3 = _get(_getPrototypeOf(1 & r5 ? t5.prototype : t5), e7, o7);
    return 2 & r5 && "function" == typeof p3 ? function(t6) {
      return p3.apply(o7, t6);
    } : p3;
  }
  var ConnectionPlugin = /* @__PURE__ */ (function(_Scope) {
    function ConnectionPlugin2() {
      var _this;
      _classCallCheck(this, ConnectionPlugin2);
      _this = _callSuper3(this, ConnectionPlugin2, ["connection"]);
      _defineProperty(_this, "presets", []);
      _defineProperty(_this, "currentFlow", null);
      _defineProperty(_this, "preudoconnection", createPseudoconnection({
        isPseudo: true
      }));
      _defineProperty(_this, "socketsCache", /* @__PURE__ */ new Map());
      return _this;
    }
    _inherits(ConnectionPlugin2, _Scope);
    return _createClass(ConnectionPlugin2, [{
      key: "addPreset",
      value: function addPreset(preset) {
        this.presets.push(preset);
      }
    }, {
      key: "findPreset",
      value: function findPreset(data) {
        var _iterator = _createForOfIteratorHelper2(this.presets), _step;
        try {
          for (_iterator.s(); !(_step = _iterator.n()).done; ) {
            var preset = _step.value;
            var flow = preset(data);
            if (flow) return flow;
          }
        } catch (err) {
          _iterator.e(err);
        } finally {
          _iterator.f();
        }
        return null;
      }
    }, {
      key: "update",
      value: function update() {
        if (!this.currentFlow) return;
        var socket = this.currentFlow.getPickedSocket();
        if (socket) {
          this.preudoconnection.render(this.areaPlugin, this.areaPlugin.area.pointer, socket);
        }
      }
      /**
       * Drop pseudo-connection if exists
       * @emits connectiondrop
       */
    }, {
      key: "drop",
      value: function drop() {
        var flowContext = {
          editor: this.editor,
          scope: this,
          socketsCache: this.socketsCache
        };
        if (this.currentFlow) {
          this.currentFlow.drop(flowContext);
          this.preudoconnection.unmount(this.areaPlugin);
          this.currentFlow = null;
        }
      }
      // eslint-disable-next-line max-statements
    }, {
      key: "pick",
      value: (function() {
        var _pick = _asyncToGenerator(/* @__PURE__ */ import_regenerator3.default.mark(function _callee(event, type) {
          var flowContext, pointedElements, pickedSocket;
          return import_regenerator3.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                flowContext = {
                  editor: this.editor,
                  scope: this,
                  socketsCache: this.socketsCache
                };
                pointedElements = elementsFromPoint(event.clientX, event.clientY);
                pickedSocket = findSocket(this.socketsCache, pointedElements);
                if (!pickedSocket) {
                  _context.next = 13;
                  break;
                }
                event.preventDefault();
                event.stopPropagation();
                this.currentFlow = this.currentFlow || this.findPreset(pickedSocket);
                if (!this.currentFlow) {
                  _context.next = 11;
                  break;
                }
                _context.next = 10;
                return this.currentFlow.pick({
                  socket: pickedSocket,
                  event: type
                }, flowContext);
              case 10:
                this.preudoconnection.mount(this.areaPlugin);
              case 11:
                _context.next = 14;
                break;
              case 13:
                if (this.currentFlow) {
                  this.currentFlow.drop(flowContext);
                }
              case 14:
                if (this.currentFlow && !this.currentFlow.getPickedSocket()) {
                  this.preudoconnection.unmount(this.areaPlugin);
                  this.currentFlow = null;
                }
                this.update();
              case 16:
              case "end":
                return _context.stop();
            }
          }, _callee, this);
        }));
        function pick(_x, _x2) {
          return _pick.apply(this, arguments);
        }
        return pick;
      })()
    }, {
      key: "setParent",
      value: function setParent(scope) {
        var _this2 = this;
        _superPropGet(ConnectionPlugin2, "setParent", this, 3)([scope]);
        this.areaPlugin = this.parentScope(BaseAreaPlugin);
        this.editor = this.areaPlugin.parentScope(NodeEditor);
        var pointerdownSocket = function pointerdownSocket2(e7) {
          void _this2.pick(e7, "down");
        };
        this.addPipe(function(context2) {
          if (!context2 || _typeof(context2) !== "object" || !("type" in context2)) return context2;
          if (context2.type === "pointermove") {
            _this2.update();
          } else if (context2.type === "pointerup") {
            void _this2.pick(context2.data.event, "up");
          } else if (context2.type === "render") {
            if (context2.data.type === "socket") {
              var element = context2.data.element;
              element.addEventListener("pointerdown", pointerdownSocket);
              _this2.socketsCache.set(element, context2.data);
            }
          } else if (context2.type === "unmount") {
            var _element = context2.data.element;
            _element.removeEventListener("pointerdown", pointerdownSocket);
            _this2.socketsCache["delete"](_element);
          }
          return context2;
        });
      }
    }]);
  })(Scope);

  // node_modules/rete-lit-plugin/dist/rete-litv-plugin.esm.js
  var import_regenerator5 = __toESM(require_regenerator2());

  // node_modules/rete-render-utils/rete-render-utils.esm.js
  var import_regenerator4 = __toESM(require_regenerator2());
  function classicConnectionPath(points, curvature) {
    var _points = _slicedToArray(points, 2), _points$ = _points[0], x1 = _points$.x, y1 = _points$.y, _points$2 = _points[1], x2 = _points$2.x, y22 = _points$2.y;
    var vertical = Math.abs(y1 - y22);
    var hx1 = x1 + Math.max(vertical / 2, Math.abs(x2 - x1)) * curvature;
    var hx2 = x2 - Math.max(vertical / 2, Math.abs(x2 - x1)) * curvature;
    return "M ".concat(x1, " ").concat(y1, " C ").concat(hx1, " ").concat(y1, " ").concat(hx2, " ").concat(y22, " ").concat(x2, " ").concat(y22);
  }
  function loopConnectionPath(points, curvature, size) {
    var _points2 = _slicedToArray(points, 2), _points2$ = _points2[0], x1 = _points2$.x, y1 = _points2$.y, _points2$2 = _points2[1], x2 = _points2$2.x, y22 = _points2$2.y;
    var k2 = y22 > y1 ? 1 : -1;
    var scale = size + Math.abs(x1 - x2) / (size / 2);
    var middleX = (x1 + x2) / 2;
    var middleY = y1 - k2 * scale;
    var vertical = (y22 - y1) * curvature;
    return "\n        M ".concat(x1, " ").concat(y1, "\n        C ").concat(x1 + scale, " ").concat(y1, "\n        ").concat(x1 + scale, " ").concat(middleY - vertical, "\n        ").concat(middleX, " ").concat(middleY, "\n        C ").concat(x2 - scale, " ").concat(middleY + vertical, "\n        ").concat(x2 - scale, " ").concat(y22, "\n        ").concat(x2, " ").concat(y22, "\n    ");
  }
  function getElementCenter(_x, _x2) {
    return _getElementCenter.apply(this, arguments);
  }
  function _getElementCenter() {
    _getElementCenter = _asyncToGenerator(/* @__PURE__ */ import_regenerator4.default.mark(function _callee(child, parent) {
      var x2, y3, currentElement, width, height;
      return import_regenerator4.default.wrap(function _callee$(_context) {
        while (1) switch (_context.prev = _context.next) {
          case 0:
            if (child.offsetParent) {
              _context.next = 5;
              break;
            }
            _context.next = 3;
            return new Promise(function(res) {
              return setTimeout(res, 0);
            });
          case 3:
            _context.next = 0;
            break;
          case 5:
            x2 = child.offsetLeft;
            y3 = child.offsetTop;
            currentElement = child.offsetParent;
            if (currentElement) {
              _context.next = 10;
              break;
            }
            throw new Error("child has null offsetParent");
          case 10:
            while (currentElement !== null && currentElement !== parent) {
              x2 += currentElement.offsetLeft + currentElement.clientLeft;
              y3 += currentElement.offsetTop + currentElement.clientTop;
              currentElement = currentElement.offsetParent;
            }
            width = child.offsetWidth;
            height = child.offsetHeight;
            return _context.abrupt("return", {
              x: x2 + width / 2,
              y: y3 + height / 2
            });
          case 14:
          case "end":
            return _context.stop();
        }
      }, _callee);
    }));
    return _getElementCenter.apply(this, arguments);
  }
  var EventEmitter = /* @__PURE__ */ (function() {
    function EventEmitter2() {
      _classCallCheck(this, EventEmitter2);
      _defineProperty(this, "listeners", /* @__PURE__ */ new Set());
    }
    return _createClass(EventEmitter2, [{
      key: "emit",
      value: function emit(data) {
        this.listeners.forEach(function(listener) {
          listener(data);
        });
      }
    }, {
      key: "listen",
      value: function listen(handler) {
        var _this = this;
        this.listeners.add(handler);
        return function() {
          _this.listeners["delete"](handler);
        };
      }
    }]);
  })();
  var SocketsPositionsStorage = /* @__PURE__ */ (function() {
    function SocketsPositionsStorage2() {
      _classCallCheck(this, SocketsPositionsStorage2);
      _defineProperty(this, "elements", /* @__PURE__ */ new Map());
    }
    return _createClass(SocketsPositionsStorage2, [{
      key: "getPosition",
      value: function getPosition(data) {
        var _found$pop$position, _found$pop;
        var list = Array.from(this.elements.values()).flat();
        var found = list.filter(function(item) {
          return item.side === data.side && item.nodeId === data.nodeId && item.key === data.key;
        });
        if (found.length > 1) console.warn(["Found more than one element for socket with same key and side.", "Probably it was not unmounted correctly"].join(" "), data);
        return (_found$pop$position = (_found$pop = found.pop()) === null || _found$pop === void 0 ? void 0 : _found$pop.position) !== null && _found$pop$position !== void 0 ? _found$pop$position : null;
      }
    }, {
      key: "add",
      value: function add(data) {
        var existing = this.elements.get(data.element);
        this.elements.set(data.element, existing ? [].concat(_toConsumableArray(existing.filter(function(n6) {
          return !(n6.nodeId === data.nodeId && n6.key === data.key && n6.side === data.side);
        })), [data]) : [data]);
      }
    }, {
      key: "remove",
      value: function remove(element) {
        this.elements["delete"](element);
      }
    }, {
      key: "snapshot",
      value: function snapshot() {
        return Array.from(this.elements.values()).flat();
      }
    }]);
  })();
  var BaseSocketPosition = /* @__PURE__ */ (function() {
    function BaseSocketPosition2() {
      _classCallCheck(this, BaseSocketPosition2);
      _defineProperty(this, "sockets", new SocketsPositionsStorage());
      _defineProperty(this, "emitter", new EventEmitter());
      _defineProperty(this, "area", null);
    }
    return _createClass(BaseSocketPosition2, [{
      key: "attach",
      value: (
        /**
         * Attach the watcher to the area's child scope.
         * @param scope Scope of the watcher that should be a child of `BaseAreaPlugin`
         */
        function attach(scope) {
          var _this = this;
          if (this.area) return;
          if (!scope.hasParent()) return;
          this.area = scope.parentScope(BaseAreaPlugin);
          this.area.addPipe(/* @__PURE__ */ (function() {
            var _ref = _asyncToGenerator(/* @__PURE__ */ import_regenerator4.default.mark(function _callee2(context2) {
              var _context$data, _nodeId, _key, _side, _element, position, _nodeId2, _context$data$payload, source, target, _nodeId3;
              return import_regenerator4.default.wrap(function _callee2$(_context2) {
                while (1) switch (_context2.prev = _context2.next) {
                  case 0:
                    if (!(context2.type === "rendered" && context2.data.type === "socket")) {
                      _context2.next = 8;
                      break;
                    }
                    _context$data = context2.data, _nodeId = _context$data.nodeId, _key = _context$data.key, _side = _context$data.side, _element = _context$data.element;
                    _context2.next = 4;
                    return _this.calculatePosition(_nodeId, _side, _key, _element);
                  case 4:
                    position = _context2.sent;
                    if (position) {
                      _this.sockets.add({
                        nodeId: _nodeId,
                        key: _key,
                        side: _side,
                        element: _element,
                        position
                      });
                      _this.emitter.emit({
                        nodeId: _nodeId,
                        key: _key,
                        side: _side
                      });
                    }
                    _context2.next = 24;
                    break;
                  case 8:
                    if (!(context2.type === "unmount")) {
                      _context2.next = 12;
                      break;
                    }
                    _this.sockets.remove(context2.data.element);
                    _context2.next = 24;
                    break;
                  case 12:
                    if (!(context2.type === "nodetranslated")) {
                      _context2.next = 16;
                      break;
                    }
                    _this.emitter.emit({
                      nodeId: context2.data.id
                    });
                    _context2.next = 24;
                    break;
                  case 16:
                    if (!(context2.type === "noderesized")) {
                      _context2.next = 23;
                      break;
                    }
                    _nodeId2 = context2.data.id;
                    _context2.next = 20;
                    return Promise.all(_this.sockets.snapshot().filter(function(item) {
                      return item.nodeId === context2.data.id && item.side === "output";
                    }).map(/* @__PURE__ */ (function() {
                      var _ref2 = _asyncToGenerator(/* @__PURE__ */ import_regenerator4.default.mark(function _callee(item) {
                        var side, key, element, position2;
                        return import_regenerator4.default.wrap(function _callee$(_context) {
                          while (1) switch (_context.prev = _context.next) {
                            case 0:
                              side = item.side, key = item.key, element = item.element;
                              _context.next = 3;
                              return _this.calculatePosition(_nodeId2, side, key, element);
                            case 3:
                              position2 = _context.sent;
                              if (position2) {
                                item.position = position2;
                              }
                            case 5:
                            case "end":
                              return _context.stop();
                          }
                        }, _callee);
                      }));
                      return function(_x2) {
                        return _ref2.apply(this, arguments);
                      };
                    })()));
                  case 20:
                    _this.emitter.emit({
                      nodeId: _nodeId2
                    });
                    _context2.next = 24;
                    break;
                  case 23:
                    if (context2.type === "render" && context2.data.type === "connection") {
                      _context$data$payload = context2.data.payload, source = _context$data$payload.source, target = _context$data$payload.target;
                      _nodeId3 = source || target;
                      _this.emitter.emit({
                        nodeId: _nodeId3
                      });
                    }
                  case 24:
                    return _context2.abrupt("return", context2);
                  case 25:
                  case "end":
                    return _context2.stop();
                }
              }, _callee2);
            }));
            return function(_x) {
              return _ref.apply(this, arguments);
            };
          })());
        }
      )
      /**
       * Listen to socket position changes. Usually used by rendering plugins to update the start/end of the connection.
       * @internal
       * @param nodeId Node ID
       * @param side Side of the socket, 'input' or 'output'
       * @param key Socket key
       * @param change Callback function that is called when the socket position changes
       */
    }, {
      key: "listen",
      value: function listen(nodeId, side, key, change) {
        var _this2 = this;
        var unlisten = this.emitter.listen(function(data) {
          if (data.nodeId !== nodeId) return;
          if ((!data.key || data.side === side) && (!data.side || data.key === key)) {
            var _this2$area;
            var position = _this2.sockets.getPosition({
              side,
              nodeId,
              key
            });
            if (!position) return;
            var x2 = position.x, y3 = position.y;
            var nodeView = (_this2$area = _this2.area) === null || _this2$area === void 0 ? void 0 : _this2$area.nodeViews.get(nodeId);
            if (nodeView) change({
              x: x2 + nodeView.position.x,
              y: y3 + nodeView.position.y
            });
          }
        });
        this.sockets.snapshot().forEach(function(data) {
          if (data.nodeId === nodeId) _this2.emitter.emit(data);
        });
        return unlisten;
      }
    }]);
  })();
  function _callSuper4(t5, o7, e7) {
    return o7 = _getPrototypeOf(o7), _possibleConstructorReturn(t5, _isNativeReflectConstruct4() ? Reflect.construct(o7, e7 || [], _getPrototypeOf(t5).constructor) : o7.apply(t5, e7));
  }
  function _isNativeReflectConstruct4() {
    try {
      var t5 = !Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
    } catch (t6) {
    }
    return (_isNativeReflectConstruct4 = function _isNativeReflectConstruct6() {
      return !!t5;
    })();
  }
  var DOMSocketPosition = /* @__PURE__ */ (function(_BaseSocketPosition) {
    function DOMSocketPosition2(props) {
      var _this;
      _classCallCheck(this, DOMSocketPosition2);
      _this = _callSuper4(this, DOMSocketPosition2);
      _this.props = props;
      return _this;
    }
    _inherits(DOMSocketPosition2, _BaseSocketPosition);
    return _createClass(DOMSocketPosition2, [{
      key: "calculatePosition",
      value: (function() {
        var _calculatePosition = _asyncToGenerator(/* @__PURE__ */ import_regenerator4.default.mark(function _callee(nodeId, side, key, element) {
          var _this$area, _this$props;
          var view, position;
          return import_regenerator4.default.wrap(function _callee$(_context) {
            while (1) switch (_context.prev = _context.next) {
              case 0:
                view = (_this$area = this.area) === null || _this$area === void 0 ? void 0 : _this$area.nodeViews.get(nodeId);
                if (view !== null && view !== void 0 && view.element) {
                  _context.next = 3;
                  break;
                }
                return _context.abrupt("return", null);
              case 3:
                _context.next = 5;
                return getElementCenter(element, view.element);
              case 5:
                position = _context.sent;
                if (!((_this$props = this.props) !== null && _this$props !== void 0 && _this$props.offset)) {
                  _context.next = 8;
                  break;
                }
                return _context.abrupt("return", this.props.offset(position, nodeId, side, key));
              case 8:
                return _context.abrupt("return", {
                  x: position.x + 12 * (side === "input" ? -1 : 1),
                  y: position.y
                });
              case 9:
              case "end":
                return _context.stop();
            }
          }, _callee, this);
        }));
        function calculatePosition(_x, _x2, _x3, _x4) {
          return _calculatePosition.apply(this, arguments);
        }
        return calculatePosition;
      })()
    }]);
  })(BaseSocketPosition);
  function getDOMSocketPosition(props) {
    return new DOMSocketPosition(props);
  }

  // node_modules/@babel/runtime/helpers/esm/taggedTemplateLiteral.js
  function _taggedTemplateLiteral(e7, t5) {
    return t5 || (t5 = e7.slice(0)), Object.freeze(Object.defineProperties(e7, {
      raw: {
        value: Object.freeze(t5)
      }
    }));
  }

  // node_modules/@babel/runtime/helpers/esm/toArray.js
  function _toArray(r5) {
    return _arrayWithHoles(r5) || _iterableToArray(r5) || _unsupportedIterableToArray2(r5) || _nonIterableRest();
  }

  // node_modules/@lit/reactive-element/decorators/custom-element.js
  var t3 = (t5) => (e7, o7) => {
    void 0 !== o7 ? o7.addInitializer(() => {
      customElements.define(t5, e7);
    }) : customElements.define(t5, e7);
  };

  // node_modules/@lit/reactive-element/decorators/property.js
  var o5 = { attribute: true, type: String, converter: u, reflect: false, hasChanged: f };
  var r4 = (t5 = o5, e7, r5) => {
    const { kind: n6, metadata: i7 } = r5;
    let s4 = globalThis.litPropertyMetadata.get(i7);
    if (void 0 === s4 && globalThis.litPropertyMetadata.set(i7, s4 = /* @__PURE__ */ new Map()), "setter" === n6 && ((t5 = Object.create(t5)).wrapped = true), s4.set(r5.name, t5), "accessor" === n6) {
      const { name: o7 } = r5;
      return { set(r6) {
        const n7 = e7.get.call(this);
        e7.set.call(this, r6), this.requestUpdate(o7, n7, t5, true, r6);
      }, init(e8) {
        return void 0 !== e8 && this.C(o7, void 0, t5, e8), e8;
      } };
    }
    if ("setter" === n6) {
      const { name: o7 } = r5;
      return function(r6) {
        const n7 = this[o7];
        e7.call(this, r6), this.requestUpdate(o7, n7, t5, true, r6);
      };
    }
    throw Error("Unsupported decorator location: " + n6);
  };
  function n4(t5) {
    return (e7, o7) => "object" == typeof o7 ? r4(t5, e7, o7) : ((t6, e8, o8) => {
      const r5 = e8.hasOwnProperty(o8);
      return e8.constructor.createProperty(o8, t6), r5 ? Object.getOwnPropertyDescriptor(e8, o8) : void 0;
    })(t5, e7, o7);
  }

  // node_modules/@lit/reactive-element/decorators/base.js
  var e4 = (e7, t5, c4) => (c4.configurable = true, c4.enumerable = true, Reflect.decorate && "object" != typeof t5 && Object.defineProperty(e7, t5, c4), c4);

  // node_modules/@lit/reactive-element/decorators/query.js
  function e5(e7, r5) {
    return (n6, s4, i7) => {
      const o7 = (t5) => t5.renderRoot?.querySelector(e7) ?? null;
      if (r5) {
        const { get: e8, set: r6 } = "object" == typeof s4 ? n6 : i7 ?? /* @__PURE__ */ (() => {
          const t5 = /* @__PURE__ */ Symbol();
          return { get() {
            return this[t5];
          }, set(e9) {
            this[t5] = e9;
          } };
        })();
        return e4(n6, s4, { get() {
          let t5 = e8.call(this);
          return void 0 === t5 && (t5 = o7(this), (null !== t5 || this.hasUpdated) && r6.call(this, t5)), t5;
        } });
      }
      return e4(n6, s4, { get() {
        return o7(this);
      } });
    };
  }

  // node_modules/lit-html/directive.js
  var t4 = { ATTRIBUTE: 1, CHILD: 2, PROPERTY: 3, BOOLEAN_ATTRIBUTE: 4, EVENT: 5, ELEMENT: 6 };
  var e6 = (t5) => (...e7) => ({ _$litDirective$: t5, values: e7 });
  var i5 = class {
    constructor(t5) {
    }
    get _$AU() {
      return this._$AM._$AU;
    }
    _$AT(t5, e7, i7) {
      this._$Ct = t5, this._$AM = e7, this._$Ci = i7;
    }
    _$AS(t5, e7) {
      return this.update(t5, e7);
    }
    update(t5, e7) {
      return this.render(...e7);
    }
  };

  // node_modules/lit-html/directives/style-map.js
  var n5 = "important";
  var i6 = " !" + n5;
  var o6 = e6(class extends i5 {
    constructor(t5) {
      if (super(t5), t5.type !== t4.ATTRIBUTE || "style" !== t5.name || t5.strings?.length > 2) throw Error("The `styleMap` directive must be used in the `style` attribute and must be the only part in the attribute.");
    }
    render(t5) {
      return Object.keys(t5).reduce((e7, r5) => {
        const s4 = t5[r5];
        return null == s4 ? e7 : e7 + `${r5 = r5.includes("-") ? r5 : r5.replace(/(?:^(webkit|moz|ms|o)|)(?=[A-Z])/g, "-$&").toLowerCase()}:${s4};`;
      }, "");
    }
    update(e7, [r5]) {
      const { style: s4 } = e7.element;
      if (void 0 === this.ft) return this.ft = new Set(Object.keys(r5)), this.render(r5);
      for (const t5 of this.ft) null == r5[t5] && (this.ft.delete(t5), t5.includes("-") ? s4.removeProperty(t5) : s4[t5] = null);
      for (const t5 in r5) {
        const e8 = r5[t5];
        if (null != e8) {
          this.ft.add(t5);
          const r6 = "string" == typeof e8 && e8.endsWith(i6);
          t5.includes("-") || r6 ? s4.setProperty(t5, r6 ? e8.slice(0, -11) : e8, r6 ? n5 : "") : s4[t5] = e8;
        }
      }
      return E;
    }
  });

  // node_modules/rete-lit-plugin/dist/rete-litv-plugin.esm.js
  function ownKeys$6(object, enumerableOnly) {
    var keys2 = Object.keys(object);
    if (Object.getOwnPropertySymbols) {
      var symbols = Object.getOwnPropertySymbols(object);
      enumerableOnly && (symbols = symbols.filter(function(sym) {
        return Object.getOwnPropertyDescriptor(object, sym).enumerable;
      })), keys2.push.apply(keys2, symbols);
    }
    return keys2;
  }
  function _objectSpread$6(target) {
    for (var i7 = 1; i7 < arguments.length; i7++) {
      var source = null != arguments[i7] ? arguments[i7] : {};
      i7 % 2 ? ownKeys$6(Object(source), true).forEach(function(key) {
        _defineProperty(target, key, source[key]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(target, Object.getOwnPropertyDescriptors(source)) : ownKeys$6(Object(source)).forEach(function(key) {
        Object.defineProperty(target, key, Object.getOwnPropertyDescriptor(source, key));
      });
    }
    return target;
  }
  function setProps(element, payload) {
    for (var _i = 0, _Object$keys = Object.keys(payload); _i < _Object$keys.length; _i++) {
      var key = _Object$keys[_i];
      element[key] = payload[key];
    }
  }
  function create(element, component, payload, onRendered) {
    var state = payload;
    var app = new component();
    app.addEventListener("updated", function() {
      onRendered();
    });
    setProps(app, payload);
    element.appendChild(app);
    onRendered();
    return {
      app,
      payload: state
    };
  }
  function _update(instance, payload) {
    instance.payload = _objectSpread$6(_objectSpread$6({}, instance.payload), payload);
    setProps(instance.app, instance.payload);
    instance.app.requestUpdate();
  }
  function destroy(instance) {
    instance.app.remove();
  }
  function getRenderer() {
    var instances = /* @__PURE__ */ new Map();
    return {
      get: function get(element) {
        return instances.get(element);
      },
      mount: function mount(element, litComponent, payload, onRendered) {
        var app = create(element, litComponent, payload, onRendered);
        instances.set(element, app);
        return app;
      },
      update: function update(app, payload) {
        _update(app, payload);
      },
      unmount: function unmount(element) {
        var app = instances.get(element);
        if (app) {
          destroy(app);
          instances["delete"](element);
        }
      }
    };
  }
  var _templateObject$h;
  var _templateObject2$f;
  function _createSuper$g(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$g();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$g() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$f(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$f();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$f(r5.d.map(_createElementDescriptor$f)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$f() {
    _getDecoratorsApi$f = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$f(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$f(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$f(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$f(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$f(def) {
    var key = _toPropertyKey$f(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$f(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$f(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$f(element.descriptor) || _isDataDescriptor$f(other.descriptor)) {
          if (_hasDecorators$f(element) || _hasDecorators$f(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$f(element)) {
            if (_hasDecorators$f(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$f(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$f(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$f(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$f(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$f(arg) {
    var key = _toPrimitive$f(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$f(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var Connection3 = _decorate$f([t3("connection-component")], function(_initialize, _LitElement) {
    var Connection4 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Connection5, _LitElement2);
      var _super = _createSuper$g(Connection5);
      function Connection5() {
        var _this;
        _classCallCheck(this, Connection5);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Connection5);
    })(_LitElement);
    return {
      F: Connection4,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "data",
        value: function value() {
          return {};
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "start",
        value: function value() {
          return {};
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "end",
        value: function value() {
          return {};
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: String
        })],
        key: "path",
        value: function value() {
          return "";
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject$h || (_templateObject$h = _taggedTemplateLiteral(["\n    svg {\n      overflow: visible !important;\n      position: absolute;\n      pointer-events: none;\n      width: 9999px;\n      height: 9999px;\n    }\n    path {\n      fill: none;\n      stroke-width: 5px;\n      stroke: steelblue;\n      pointer-events: auto;\n    }\n  "])));
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          return b2(_templateObject2$f || (_templateObject2$f = _taggedTemplateLiteral(['\n      <svg data-testid="connection">\n        <path d=', "></path>\n      </svg>\n    "])), this.path);
        }
      }]
    };
  }, i4);
  var _templateObject$g;
  var _templateObject2$e;
  function _createSuper$f(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$f();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$f() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$e(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$e();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$e(r5.d.map(_createElementDescriptor$e)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$e() {
    _getDecoratorsApi$e = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$e(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$e(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$e(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$e(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$e(def) {
    var key = _toPropertyKey$e(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$e(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$e(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$e(element.descriptor) || _isDataDescriptor$e(other.descriptor)) {
          if (_hasDecorators$e(element) || _hasDecorators$e(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$e(element)) {
            if (_hasDecorators$e(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$e(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$e(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$e(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$e(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$e(arg) {
    var key = _toPrimitive$e(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$e(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var ConnectionWrapper = _decorate$e([t3("connection-wrapper")], function(_initialize, _LitElement) {
    var ConnectionWrapper2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(ConnectionWrapper3, _LitElement2);
      var _super = _createSuper$f(ConnectionWrapper3);
      function ConnectionWrapper3() {
        var _this;
        _classCallCheck(this, ConnectionWrapper3);
        _this = _super.call(this);
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(ConnectionWrapper3);
    })(_LitElement);
    return {
      F: ConnectionWrapper2,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "component",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "data",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "start",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "end",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "path",
        value: function value() {
          return function() {
            throw new Error("PATH FUNCTION NOT SET PROPERLY");
          };
        }
      }, {
        kind: "field",
        key: "observedStart",
        value: function value() {
          return {
            x: 0,
            y: 0
          };
        }
      }, {
        kind: "field",
        key: "observedEnd",
        value: function value() {
          return {
            x: 0,
            y: 0
          };
        }
      }, {
        kind: "field",
        key: "observedPath",
        value: function value() {
          return "";
        }
      }, {
        kind: "field",
        key: "onDestroy",
        value: function value() {
          return null;
        }
      }, {
        kind: "method",
        key: "connectedCallback",
        value: function connectedCallback() {
          _get(_getPrototypeOf(ConnectionWrapper2.prototype), "connectedCallback", this).call(this);
          this.initializeWatchers();
          this.fetchPath();
        }
      }, {
        kind: "method",
        key: "disconnectedCallback",
        value: function disconnectedCallback() {
          if (this.onDestroy) this.onDestroy();
          _get(_getPrototypeOf(ConnectionWrapper2.prototype), "disconnectedCallback", this).call(this);
        }
      }, {
        kind: "method",
        key: "initializeWatchers",
        value: function initializeWatchers() {
          var _this2 = this;
          var unwatch1 = typeof this.start === "function" && this.start(function(pos) {
            _this2.observedStart = pos;
            _this2.fetchPath();
          });
          var unwatch2 = typeof this.end === "function" && this.end(function(pos) {
            _this2.observedEnd = pos;
            _this2.fetchPath();
          });
          this.onDestroy = function() {
            unwatch1 && unwatch1();
            unwatch2 && unwatch2();
          };
        }
      }, {
        kind: "method",
        key: "fetchPath",
        value: (function() {
          var _fetchPath = _asyncToGenerator(/* @__PURE__ */ import_regenerator5.default.mark(function _callee() {
            return import_regenerator5.default.wrap(function _callee$(_context) {
              while (1) switch (_context.prev = _context.next) {
                case 0:
                  if (!(this.startPosition && this.endPosition)) {
                    _context.next = 5;
                    break;
                  }
                  _context.next = 3;
                  return this.path(this.startPosition, this.endPosition);
                case 3:
                  this.observedPath = _context.sent;
                  this.requestUpdate();
                // Request re-render
                case 5:
                case "end":
                  return _context.stop();
              }
            }, _callee, this);
          }));
          function fetchPath() {
            return _fetchPath.apply(this, arguments);
          }
          return fetchPath;
        })()
      }, {
        kind: "get",
        key: "startPosition",
        value: function startPosition() {
          return this.start && "x" in this.start ? this.start : this.observedStart;
        }
      }, {
        kind: "get",
        key: "endPosition",
        value: function endPosition() {
          return this.end && "x" in this.end ? this.end : this.observedEnd;
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          if (this.component) {
            var dynamicComponent = new this.component();
            dynamicComponent.data = this.data;
            dynamicComponent.start = this.startPosition;
            dynamicComponent.end = this.endPosition;
            dynamicComponent.path = this.observedPath;
            return b2(_templateObject$g || (_templateObject$g = _taggedTemplateLiteral(["", ""])), dynamicComponent);
          }
          return b2(_templateObject2$e || (_templateObject2$e = _taggedTemplateLiteral([""])));
        }
      }]
    };
  }, i4);
  var _templateObject$f;
  var vars$1 = i(_templateObject$f || (_templateObject$f = _taggedTemplateLiteral([":host {\n    --node-color: rgba(110, 136, 255, 0.8);\n    --node-color-selected: #ffd92c;\n    --group-color: rgba(15, 80, 255, 0.2);\n    --group-handler-size: 40px;\n    --group-handler-offset: -10px;\n    --socket-size: 24px;\n    --socket-margin: 6px;\n    --socket-color: #96b38a;\n    --node-width: 180px;\n}"])));
  var _templateObject$e;
  var _templateObject2$d;
  function _createSuper$e(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$e();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$e() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$d(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$d();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$d(r5.d.map(_createElementDescriptor$d)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$d() {
    _getDecoratorsApi$d = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$d(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$d(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$d(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$d(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$d(def) {
    var key = _toPropertyKey$d(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$d(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$d(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$d(element.descriptor) || _isDataDescriptor$d(other.descriptor)) {
          if (_hasDecorators$d(element) || _hasDecorators$d(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$d(element)) {
            if (_hasDecorators$d(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$d(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$d(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$d(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$d(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$d(arg) {
    var key = _toPrimitive$d(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$d(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var Control3 = _decorate$d(null, function(_initialize, _LitElement) {
    var Control4 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Control5, _LitElement2);
      var _super = _createSuper$e(Control5);
      function Control5() {
        var _this;
        _classCallCheck(this, Control5);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Control5);
    })(_LitElement);
    return {
      F: Control4,
      d: [{
        kind: "field",
        decorators: [n4({
          type: classic.InputControl
        })],
        key: "data",
        value: void 0
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return [vars$1, i(_templateObject$e || (_templateObject$e = _taggedTemplateLiteral(["\n    input {\n      width: 100%;\n      border-radius: 30px;\n      background-color: white;\n      padding: 2px 6px;\n      border: 1px solid #999;\n      font-size: 110%;\n      box-sizing: border-box;\n    }\n  "])))];
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          var _this$data, _this$data2, _this$data3;
          return b2(_templateObject2$d || (_templateObject2$d = _taggedTemplateLiteral(['\n      <input\n        type="', '"\n        .value="', '"\n        ?readonly="', '"\n        @input="', '"\n        @pointerdown="', '"\n      />\n    '])), (_this$data = this.data) === null || _this$data === void 0 ? void 0 : _this$data.type, (_this$data2 = this.data) === null || _this$data2 === void 0 ? void 0 : _this$data2.value, (_this$data3 = this.data) === null || _this$data3 === void 0 ? void 0 : _this$data3.readonly, this.change, this.stopEvent);
        }
      }, {
        kind: "method",
        key: "change",
        value: function change(e7) {
          var _this$data4, _e$target, _e$target2, _this$data5;
          var val = ((_this$data4 = this.data) === null || _this$data4 === void 0 ? void 0 : _this$data4.type) === "number" ? +((_e$target = e7.target) === null || _e$target === void 0 ? void 0 : _e$target.value) : (_e$target2 = e7.target) === null || _e$target2 === void 0 ? void 0 : _e$target2.value;
          (_this$data5 = this.data) === null || _this$data5 === void 0 ? void 0 : _this$data5.setValue(val);
        }
      }, {
        kind: "method",
        key: "stopEvent",
        value: function stopEvent(e7) {
          e7.stopPropagation();
        }
      }]
    };
  }, i4);
  customElements.define("control-component", Control3);
  var _templateObject$d;
  var _templateObject2$c;
  var _templateObject3$5;
  var _templateObject4$3;
  var _templateObject5;
  function _createSuper$d(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$d();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$d() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$c(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$c();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$c(r5.d.map(_createElementDescriptor$c)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$c() {
    _getDecoratorsApi$c = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$c(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$c(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$c(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$c(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$c(def) {
    var key = _toPropertyKey$c(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$c(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$c(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$c(element.descriptor) || _isDataDescriptor$c(other.descriptor)) {
          if (_hasDecorators$c(element) || _hasDecorators$c(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$c(element)) {
            if (_hasDecorators$c(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$c(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$c(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$c(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$c(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$c(arg) {
    var key = _toPrimitive$c(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$c(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  function sortByIndex() {
    var entries = arguments.length > 0 && arguments[0] !== void 0 ? arguments[0] : [];
    entries.sort(function(a3, b3) {
      var ai = a3[1] && a3[1].index || 0;
      var bi = b3[1] && b3[1].index || 0;
      return ai - bi;
    });
    return entries;
  }
  var Node2 = _decorate$c([t3("node-component")], function(_initialize, _LitElement) {
    var Node3 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Node4, _LitElement2);
      var _super = _createSuper$d(Node4);
      function Node4() {
        var _this;
        _classCallCheck(this, Node4);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Node4);
    })(_LitElement);
    return {
      F: Node3,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "data",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "emit",
        value: function value() {
          return function() {
            throw new Error("emit not set properly");
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: String
        })],
        key: "seed",
        value: function value() {
          return "";
        }
      }, {
        kind: "method",
        key: "nodeStyles",
        value: function nodeStyles() {
          var _this$data, _this$data2;
          return "width: ".concat(((_this$data = this.data) === null || _this$data === void 0 ? void 0 : _this$data.width) || "100px", "; height: ").concat(
            // @ts-ignore
            ((_this$data2 = this.data) === null || _this$data2 === void 0 ? void 0 : _this$data2.height) || "100px"
          );
        }
      }, {
        kind: "method",
        key: "inputs",
        value: function inputs() {
          var _this$data3;
          return sortByIndex(Object.entries(((_this$data3 = this.data) === null || _this$data3 === void 0 ? void 0 : _this$data3.inputs) || {}));
        }
      }, {
        kind: "method",
        key: "controls",
        value: function controls() {
          var _this$data4;
          return sortByIndex(Object.entries(((_this$data4 = this.data) === null || _this$data4 === void 0 ? void 0 : _this$data4.controls) || {}));
        }
      }, {
        kind: "method",
        key: "outputs",
        value: function outputs() {
          var _this$data5;
          return sortByIndex(Object.entries(((_this$data5 = this.data) === null || _this$data5 === void 0 ? void 0 : _this$data5.outputs) || {}));
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return [vars$1, i(_templateObject$d || (_templateObject$d = _taggedTemplateLiteral(["\n            :host {\n                display: block;\n            }\n\n            .node {\n                background: var(--node-color);\n                border: 2px solid #4e58bf;\n                border-radius: 10px;\n                cursor: pointer;\n                box-sizing: border-box;\n                width: var(--node-width);\n                height: auto;\n                padding-bottom: 6px;\n                position: relative;\n                user-select: none;\n                line-height: initial;\n                font-family: Arial;\n            }\n\n            .node:hover {\n                background: linear-gradient(\n                        rgba(255, 255, 255, 0.04),\n                        rgba(255, 255, 255, 0.04)\n                    ),\n                    var(--node-color);\n            }\n\n            .node.selected {\n                background: var(--node-color-selected);\n                border-color: #e3c000;\n            }\n\n            .node .title {\n                color: white;\n                font-family: sans-serif;\n                font-size: 18px;\n                padding: 8px;\n            }\n\n            .node .output {\n                text-align: right;\n            }\n            .node .input {\n                text-alight: left;\n            }\n\n            .node .input-socket {\n                text-align: left;\n                margin-left: calc(\n                    (var(--socket-size) / -2) - var(--socket-margin)\n                );\n                display: inline-block;\n            }\n\n            .node .output-socket {\n                text-align: right;\n                margin-right: calc(\n                    (var(--socket-size) / -2) - var(--socket-margin)\n                );\n                display: inline-block;\n            }\n\n            .node .input-title,\n            .node .output-title {\n                vertical-align: middle;\n                color: white;\n                display: inline-block;\n                font-family: sans-serif;\n                font-size: 14px;\n                margin: var(--socket-margin);\n                line-height: var(--socket-size);\n            }\n\n            .node .input-control {\n                z-index: 1;\n                width: calc(\n                    100% - var(--socket-size) - 2 * var(--socket-margin)\n                );\n                vertical-align: middle;\n                display: inline-block;\n            }\n\n            .node .input-title[hidden],\n            .node .output-title[hidden] {\n                display: none;\n            }\n\n            .node .control {\n                padding: var(--socket-margin)\n                    calc(var(--socket-size) / 2 + var(--socket-margin));\n                display: block;\n                margin: -4px;\n            }\n        "])))];
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          var _this$data6, _this$data7, _this2 = this;
          return b2(_templateObject2$c || (_templateObject2$c = _taggedTemplateLiteral(['\n            <div\n                class="node ', '"\n                style=', '\n                data-testid="node"\n            >\n                <div class="title" data-testid="title">', "</div>\n\n                <!-- Outputs -->\n                ", "\n\n                <!-- Controls -->\n                ", "\n\n                <!-- Inputs -->\n                ", "\n            </div>\n        "])), (_this$data6 = this.data) !== null && _this$data6 !== void 0 && _this$data6.selected ? "selected" : "", this.nodeStyles(), (_this$data7 = this.data) === null || _this$data7 === void 0 ? void 0 : _this$data7.label, this.outputs().map(function(_ref) {
            var _this2$data;
            var _ref2 = _slicedToArray(_ref, 2), key = _ref2[0], output = _ref2[1];
            return b2(_templateObject3$5 || (_templateObject3$5 = _taggedTemplateLiteral(['\n                        <div class="output" data-testid="output-', '">\n                            <div\n                                class="output-title"\n                                data-testid="output-title"\n                            >\n                                ', '\n                            </div>\n                            <ref-element\n                                class="output-socket"\n                                .data=', "\n                                .emit=", '\n                                data-testid="output-socket"\n                            ></ref-element>\n                        </div>\n                    '])), key, output.label, {
              type: "socket",
              side: "output",
              key,
              nodeId: (_this2$data = _this2.data) === null || _this2$data === void 0 ? void 0 : _this2$data.id,
              payload: output.socket
            }, _this2.emit);
          }), this.controls().map(function(_ref3) {
            var _ref4 = _slicedToArray(_ref3, 2), key = _ref4[0], control = _ref4[1];
            return b2(_templateObject4$3 || (_templateObject4$3 = _taggedTemplateLiteral(['\n                        <ref-element\n                            data-testid="control-', '"\n                            class="control"\n                            .emit=', "\n                            .data=", "\n                        ></ref-element>\n                    "])), key, _this2.emit, {
              type: "control",
              payload: control
            });
          }), this.inputs().map(function(_ref5) {
            var _this2$data2;
            var _ref6 = _slicedToArray(_ref5, 2), key = _ref6[0], input = _ref6[1];
            return b2(_templateObject5 || (_templateObject5 = _taggedTemplateLiteral(['\n                        <div class="input" data-testid="input-', '">\n                            <ref-element\n                                class="input-socket"\n                                .data=', "\n                                .emit=", '\n                                data-testid="input-socket"\n                            ></ref-element>\n                            <div\n                                class="input-title"\n                                ?hidden=', '\n                                data-testid="input-title"\n                            >\n                                ', "\n                            </div>\n                            <ref-element\n                                ?hidden=", "\n                                .emit=", "\n                                .data=", '\n                                data-testid="input-control"\n                                class="input-control"\n                            ></ref-element>\n                        </div>\n                    '])), key, {
              type: "socket",
              side: "input",
              key,
              nodeId: (_this2$data2 = _this2.data) === null || _this2$data2 === void 0 ? void 0 : _this2$data2.id,
              payload: input.socket
            }, _this2.emit, input.control && input.showControl, input.label, !input.control && input.showControl, _this2.emit, {
              type: "control",
              payload: input.control
            });
          }));
        }
      }]
    };
  }, i4);
  var _templateObject$c;
  var _templateObject2$b;
  function _createSuper$c(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$c();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$c() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$b(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$b();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$b(r5.d.map(_createElementDescriptor$b)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$b() {
    _getDecoratorsApi$b = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$b(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$b(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$b(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$b(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$b(def) {
    var key = _toPropertyKey$b(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$b(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$b(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$b(element.descriptor) || _isDataDescriptor$b(other.descriptor)) {
          if (_hasDecorators$b(element) || _hasDecorators$b(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$b(element)) {
            if (_hasDecorators$b(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$b(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$b(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$b(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$b(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$b(arg) {
    var key = _toPrimitive$b(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$b(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var Socket3 = _decorate$b([t3("socket-component")], function(_initialize, _LitElement) {
    var Socket4 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Socket5, _LitElement2);
      var _super = _createSuper$c(Socket5);
      function Socket5() {
        var _this;
        _classCallCheck(this, Socket5);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Socket5);
    })(_LitElement);
    return {
      F: Socket4,
      d: [{
        kind: "field",
        decorators: [n4({
          type: classic.Socket
        })],
        key: "data",
        value: void 0
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return [vars$1, i(_templateObject$c || (_templateObject$c = _taggedTemplateLiteral(["\n            :host {\n                display: inline-block;\n                cursor: pointer;\n                border: 1px solid white;\n                border-radius: calc(var(--socket-size) / 2);\n                width: var(--socket-size);\n                height: var(--socket-size);\n                margin: var(--socket-margin);\n                vertical-align: middle;\n                background: var(--socket-color);\n                z-index: 2;\n                box-sizing: border-box;\n            }\n            :host(:hover) {\n                border-width: 4px;\n            }\n            :host(.multiple) {\n                border-color: yellow;\n            }\n            :host(.output) {\n                margin-right: calc(var(--socket-size) / -2);\n            }\n            :host(.input) {\n                margin-left: calc(var(--socket-size) / -2);\n            }\n        "])))];
        }
      }, {
        kind: "method",
        key: "handleClick",
        value: (
          // Define the click handler function
          function handleClick() {
            console.log("Socket has been clicked");
          }
        )
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          var _this$data;
          return b2(_templateObject2$b || (_templateObject2$b = _taggedTemplateLiteral(['\n            <div\n                class="socket"\n                title="', '"\n                @click="', '"\n            ></div>\n        '])), (_this$data = this.data) === null || _this$data === void 0 ? void 0 : _this$data.name, this.handleClick);
        }
      }]
    };
  }, i4);
  function ownKeys$5(object, enumerableOnly) {
    var keys2 = Object.keys(object);
    if (Object.getOwnPropertySymbols) {
      var symbols = Object.getOwnPropertySymbols(object);
      enumerableOnly && (symbols = symbols.filter(function(sym) {
        return Object.getOwnPropertyDescriptor(object, sym).enumerable;
      })), keys2.push.apply(keys2, symbols);
    }
    return keys2;
  }
  function _objectSpread$5(target) {
    for (var i7 = 1; i7 < arguments.length; i7++) {
      var source = null != arguments[i7] ? arguments[i7] : {};
      i7 % 2 ? ownKeys$5(Object(source), true).forEach(function(key) {
        _defineProperty(target, key, source[key]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(target, Object.getOwnPropertyDescriptors(source)) : ownKeys$5(Object(source)).forEach(function(key) {
        Object.defineProperty(target, key, Object.getOwnPropertyDescriptor(source, key));
      });
    }
    return target;
  }
  function setup$3(props) {
    var positionWatcher = typeof (props === null || props === void 0 ? void 0 : props.socketPositionWatcher) === "undefined" ? getDOMSocketPosition() : props === null || props === void 0 ? void 0 : props.socketPositionWatcher;
    var _ref = (props === null || props === void 0 ? void 0 : props.customize) || {}, node = _ref.node, connection = _ref.connection, socket = _ref.socket, control = _ref.control;
    return {
      attach: function attach(plugin) {
        positionWatcher.attach(plugin);
      },
      update: function update(context2, plugin) {
        var payload = context2.data.payload;
        var parent = plugin.parentScope();
        if (!parent) throw new Error("parent");
        var emit = parent.emit.bind(parent);
        if (context2.data.type === "node") {
          return {
            data: payload,
            emit
          };
        } else if (context2.data.type === "connection") {
          var _context$data = context2.data, start = _context$data.start, end = _context$data.end;
          return _objectSpread$5(_objectSpread$5({
            data: payload
          }, start ? {
            start
          } : {}), end ? {
            end
          } : {});
        }
        return {
          data: payload
        };
      },
      // eslint-disable-next-line max-statements, complexity
      render: function render(context2, plugin) {
        var parent = plugin.parentScope();
        var emit = parent.emit.bind(parent);
        if (context2.data.type === "node") {
          var component = node ? node(context2.data) : Node2;
          return component && {
            component,
            props: {
              data: context2.data.payload,
              emit
            }
          };
        } else if (context2.data.type === "connection") {
          var _component = connection ? connection(context2.data) : Connection3;
          var payload = context2.data.payload;
          var source = payload.source, target = payload.target, sourceOutput = payload.sourceOutput, targetInput = payload.targetInput;
          return _component && {
            component: ConnectionWrapper,
            props: {
              data: context2.data.payload,
              component: _component,
              start: context2.data.start || function(change) {
                return positionWatcher.listen(source, "output", sourceOutput, change);
              },
              end: context2.data.end || function(change) {
                return positionWatcher.listen(target, "input", targetInput, change);
              },
              path: (function() {
                var _path = _asyncToGenerator(/* @__PURE__ */ import_regenerator5.default.mark(function _callee(start, end) {
                  var response, _response$data, path2, points, curvature;
                  return import_regenerator5.default.wrap(function _callee$(_context) {
                    while (1) switch (_context.prev = _context.next) {
                      case 0:
                        _context.next = 2;
                        return plugin.emit({
                          type: "connectionpath",
                          data: {
                            payload,
                            points: [start, end]
                          }
                        });
                      case 2:
                        response = _context.sent;
                        if (response) {
                          _context.next = 5;
                          break;
                        }
                        return _context.abrupt("return", "");
                      case 5:
                        _response$data = response.data, path2 = _response$data.path, points = _response$data.points;
                        curvature = 0.3;
                        if (!(!path2 && points.length !== 2)) {
                          _context.next = 9;
                          break;
                        }
                        throw new Error("cannot render connection with a custom number of points");
                      case 9:
                        if (path2) {
                          _context.next = 11;
                          break;
                        }
                        return _context.abrupt("return", payload.isLoop ? loopConnectionPath(points, curvature, 120) : classicConnectionPath(points, curvature));
                      case 11:
                        return _context.abrupt("return", path2);
                      case 12:
                      case "end":
                        return _context.stop();
                    }
                  }, _callee);
                }));
                function path(_x, _x2) {
                  return _path.apply(this, arguments);
                }
                return path;
              })()
            }
          };
        } else if (context2.data.type === "socket") {
          var _payload = context2.data.payload;
          var _component2 = socket ? socket(context2.data) : Socket3;
          return {
            component: _component2,
            props: {
              data: _payload
            }
          };
        } else if (context2.data.type === "control") {
          var _payload2 = context2.data.payload;
          if (control) {
            var _component3 = control(context2.data);
            return _component3 && {
              component: _component3,
              props: {
                data: _payload2
              }
            };
          }
          return context2.data.payload instanceof classic.InputControl ? {
            component: Control3,
            props: {
              data: _payload2
            }
          } : null;
        }
      }
    };
  }
  var index$4 = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    setup: setup$3,
    Connection: Connection3,
    Control: Control3,
    Node: Node2,
    Socket: Socket3
  });
  var _templateObject$b;
  var vars = i(_templateObject$b || (_templateObject$b = _taggedTemplateLiteral([":host {\n    --context-color: rgba(110, 136, 255, 0.8);\n    --context-color-light: rgba(130, 153, 255, 0.8);\n    --context-color-dark: rgba(69, 103, 255, 0.8);\n    --context-menu-round: 5px;\n    --width: 120px;\n    --test: 23px;\n}"])));
  var _templateObject$a;
  var _templateObject2$a;
  function _createSuper$b(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$b();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$b() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$a(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$a();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$a(r5.d.map(_createElementDescriptor$a)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$a() {
    _getDecoratorsApi$a = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$a(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$a(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$a(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$a(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$a(def) {
    var key = _toPropertyKey$a(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$a(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$a(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$a(element.descriptor) || _isDataDescriptor$a(other.descriptor)) {
          if (_hasDecorators$a(element) || _hasDecorators$a(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$a(element)) {
            if (_hasDecorators$a(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$a(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$a(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$a(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$a(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$a(arg) {
    var key = _toPrimitive$a(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$a(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  _decorate$a([t3("block-component")], function(_initialize, _LitElement) {
    var Block = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Block2, _LitElement2);
      var _super = _createSuper$b(Block2);
      function Block2() {
        var _this;
        _classCallCheck(this, Block2);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Block2);
    })(_LitElement);
    return {
      F: Block,
      d: [{
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return [vars, i(_templateObject$a || (_templateObject$a = _taggedTemplateLiteral(["\n    :host {\n      color: #fff;\n      padding: 4px;\n      border-bottom: 1px solid var(--context-color-dark);\n      background-color: var(--context-color);\n      cursor: pointer;\n      box-sizing: border-box;\n      width: 100%;\n      position: relative;\n    }\n\n    :host(:first-child) {\n      border-top-left-radius: var(--context-menu-round);\n      border-top-right-radius: var(--context-menu-round);\n    }\n\n    :host(:last-child) {\n      border-bottom-left-radius: var(--context-menu-round);\n      border-bottom-right-radius: var(--context-menu-round);\n    }\n\n    :host(:hover) {\n      background-color: var(--context-color-light);\n    }\n  "])))];
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          return b2(_templateObject2$a || (_templateObject2$a = _taggedTemplateLiteral(["\n      <slot></slot>\n    "])));
        }
      }]
    };
  }, i4);
  function debounce(delay, cb) {
    return {
      timeout: null,
      cancel: function cancel() {
        if (this.timeout) {
          window.clearTimeout(this.timeout);
          this.timeout = null;
        }
      },
      call: function call() {
        this.timeout = window.setTimeout(function() {
          cb();
        }, delay);
      }
    };
  }
  var _templateObject$9;
  var _templateObject2$9;
  var _templateObject3$4;
  var _templateObject4$2;
  function _createSuper$a(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$a();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$a() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$9(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$9();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$9(r5.d.map(_createElementDescriptor$9)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$9() {
    _getDecoratorsApi$9 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$9(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$9(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$9(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$9(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$9(def) {
    var key = _toPropertyKey$9(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$9(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$9(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$9(element.descriptor) || _isDataDescriptor$9(other.descriptor)) {
          if (_hasDecorators$9(element) || _hasDecorators$9(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$9(element)) {
            if (_hasDecorators$9(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$9(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$9(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$9(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$9(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$9(arg) {
    var key = _toPrimitive$9(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$9(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  _decorate$9([t3("item-component")], function(_initialize, _LitElement) {
    var Item = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Item2, _LitElement2);
      var _super = _createSuper$a(Item2);
      function Item2() {
        var _this;
        _classCallCheck(this, Item2);
        _this = _super.call(this);
        _initialize(_assertThisInitialized(_this));
        _this.hide = debounce(_this.delay, _this.hideSubitems.bind(_assertThisInitialized(_this)));
        return _this;
      }
      return _createClass(Item2);
    })(_LitElement);
    return {
      F: Item,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Array
        })],
        key: "subitems",
        value: function value() {
          return [];
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "delay",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Boolean
        })],
        key: "visibleSubitems",
        value: function value() {
          return false;
        }
      }, {
        kind: "field",
        key: "hide",
        value: void 0
      }, {
        kind: "method",
        key: "hideSubitems",
        value: function hideSubitems() {
          this.visibleSubitems = false;
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          var _this2 = this;
          return b2(_templateObject$9 || (_templateObject$9 = _taggedTemplateLiteral(['\n      <block-component class="block ', '" data-testid="context-menu-item">\n        <div \n          class="content"\n          @click="', '"\n          @wheel="', '"\n          @pointerover="', '"\n          @pointerleave="', '"\n          @pointerdown="', '"\n        >\n          <slot></slot>\n          ', "\n        </div>\n      </block-component>\n    "])), this.subitems ? "hasSubitems" : "", function(event) {
            _this2.dispatchEvent(new CustomEvent("select", {
              detail: event
            }));
            _this2.dispatchEvent(new CustomEvent("hide"));
          }, function(event) {
            return event.stopPropagation();
          }, function() {
            _this2.hide.cancel();
            _this2.visibleSubitems = true;
          }, function() {
            return _this2.hide.call(_this2);
          }, function(event) {
            return event.stopPropagation();
          }, this.subitems && this.visibleSubitems ? b2(_templateObject2$9 || (_templateObject2$9 = _taggedTemplateLiteral(['\n            <div class="subitems">\n              ', "\n            </div>\n          "])), this.subitems.map(function(item) {
            return b2(_templateObject3$4 || (_templateObject3$4 = _taggedTemplateLiteral(['\n                <item-component\n                  @select="', '"\n                  .delay="', '"\n                  @hide="', '"\n                  .subitems="', '"\n                >', "</item-component>\n              "])), function(event) {
              return item.handler(event);
            }, _this2.delay, function() {
              return _this2.dispatchEvent(new CustomEvent("hide"));
            }, item.subitems, item.label);
          })) : "");
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return [vars, i(_templateObject4$2 || (_templateObject4$2 = _taggedTemplateLiteral(["\n\n    .block {\n      padding: 0;\n    }\n\n    .content {\n      padding: 4px;\n      background-color: var(--context-color);\n    }\n\n    .hasSubitems:after {\n      content: '\u25BA';\n      position: absolute;\n      opacity: 0.6;\n      right: 5px;\n      top: 5px;\n    }\n\n    .subitems {\n      position: absolute;\n      top: 0;\n      left: 100%;\n      width: var(--width);\n    }\n  "])))];
        }
      }]
    };
  }, i4);
  var _templateObject$8;
  var _templateObject2$8;
  function _createSuper$9(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$9();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$9() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$8(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$8();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$8(r5.d.map(_createElementDescriptor$8)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$8() {
    _getDecoratorsApi$8 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$8(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$8(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$8(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$8(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$8(def) {
    var key = _toPropertyKey$8(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$8(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$8(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$8(element.descriptor) || _isDataDescriptor$8(other.descriptor)) {
          if (_hasDecorators$8(element) || _hasDecorators$8(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$8(element)) {
            if (_hasDecorators$8(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$8(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$8(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$8(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$8(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$8(arg) {
    var key = _toPrimitive$8(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$8(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  _decorate$8([t3("search-component")], function(_initialize, _LitElement) {
    var Search = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Search2, _LitElement2);
      var _super = _createSuper$9(Search2);
      function Search2() {
        var _this;
        _classCallCheck(this, Search2);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Search2);
    })(_LitElement);
    return {
      F: Search,
      d: [{
        kind: "field",
        decorators: [n4({
          type: String
        })],
        key: "text",
        value: function value() {
          return "";
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject$8 || (_templateObject$8 = _taggedTemplateLiteral(["\n    .search {\n      color: white;\n      padding: 1px 8px;\n      border: 1px solid white;\n      border-radius: 10px;\n      font-size: 16px;\n      font-family: serif;\n      width: 100%;\n      box-sizing: border-box;\n      background: transparent;\n    }\n  "])));
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          return b2(_templateObject2$8 || (_templateObject2$8 = _taggedTemplateLiteral(['\n      <input class="search" \n        .value="', '" \n        @input="', '" \n        data-testid="context-menu-search-input">\n      </input>\n    '])), this.text, this.handleInput);
        }
      }, {
        kind: "method",
        key: "handleInput",
        value: function handleInput(e7) {
          this.text = e7.target.value;
          this.dispatchEvent(new CustomEvent("change", {
            detail: this.text
          }));
        }
      }]
    };
  }, i4);
  var _templateObject$7;
  var _templateObject2$7;
  var _templateObject3$3;
  var _templateObject4$1;
  function _createSuper$8(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$8();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$8() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$7(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$7();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$7(r5.d.map(_createElementDescriptor$7)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$7() {
    _getDecoratorsApi$7 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$7(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$7(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$7(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$7(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$7(def) {
    var key = _toPropertyKey$7(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$7(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$7(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$7(element.descriptor) || _isDataDescriptor$7(other.descriptor)) {
          if (_hasDecorators$7(element) || _hasDecorators$7(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$7(element)) {
            if (_hasDecorators$7(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$7(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$7(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$7(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$7(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$7(arg) {
    var key = _toPrimitive$7(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$7(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var Menu = _decorate$7([t3("menu-component")], function(_initialize, _LitElement) {
    var Menu2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Menu3, _LitElement2);
      var _super = _createSuper$8(Menu3);
      function Menu3() {
        var _this;
        _classCallCheck(this, Menu3);
        _this = _super.call(this);
        _initialize(_assertThisInitialized(_this));
        _this.hide = debounce(_this.delay, _this.onHide);
        return _this;
      }
      return _createClass(Menu3);
    })(_LitElement);
    return {
      F: Menu2,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Array
        })],
        key: "items",
        value: function value() {
          return [];
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "delay",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Boolean
        })],
        key: "searchBar",
        value: function value() {
          return false;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "onHide",
        value: function value() {
          return function() {
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: String
        })],
        key: "seed",
        value: function value() {
          return "";
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: String
        })],
        key: "filter",
        value: function value() {
          return "";
        }
      }, {
        kind: "field",
        key: "hide",
        value: void 0
      }, {
        kind: "method",
        key: "getItems",
        value: function getItems() {
          var filterRegexp = new RegExp(this.filter, "i");
          var filteredList = this.items.filter(function(item) {
            return item.label.match(filterRegexp);
          });
          return filteredList;
        }
      }, {
        kind: "method",
        key: "connectedCallback",
        value: function connectedCallback() {
          _get(_getPrototypeOf(Menu2.prototype), "connectedCallback", this).call(this);
        }
      }, {
        kind: "method",
        key: "disconnectedCallback",
        value: function disconnectedCallback() {
          if (this.hide) this.hide.cancel();
          _get(_getPrototypeOf(Menu2.prototype), "disconnectedCallback", this).call(this);
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          var _this2 = this;
          return b2(_templateObject$7 || (_templateObject$7 = _taggedTemplateLiteral(['\n        <div\n            class="menu"\n            @mouseover="', '"\n            @mouseleave="', '"\n            data-testid="context-menu"\n            rete-context-menu\n        >\n            ', "\n            ", "\n        </div>\n    "])), this.hide.cancel, this.hide.call, this.searchBar ? b2(_templateObject2$7 || (_templateObject2$7 = _taggedTemplateLiteral(['\n                      <block-component>\n                          <search-component\n                              .text="', '"\n                              @change="', '"\n                          ></search-component>\n                      </block-component>\n                  '])), this.filter, function(e7) {
            return _this2.filter = e7.detail;
          }) : "", this.getItems().map(function(item) {
            return b2(_templateObject3$3 || (_templateObject3$3 = _taggedTemplateLiteral(['\n                    <item-component\n                        @select="', '"\n                        .delay="', '"\n                        @hide="', '"\n                        .subitems="', '"\n                    >\n                        ', "\n                    </item-component>\n                "])), item.handler, _this2.delay, _this2.onHide, item.subitems, item.label);
          }));
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return [vars, i(_templateObject4$1 || (_templateObject4$1 = _taggedTemplateLiteral(["\n\n    .menu {\n      padding: 10px;\n      width: var(--width);\n      margin-top: -20px;\n      margin-left: calc(var(--width) / -2);\n    }\n  "])))];
        }
      }]
    };
  }, i4);
  function setup$2(props) {
    var delay = typeof (props === null || props === void 0 ? void 0 : props.delay) === "undefined" ? 1e3 : props.delay;
    return {
      update: function update(context2) {
        if (context2.data.type === "contextmenu") {
          return {
            items: context2.data.items,
            delay,
            searchBar: context2.data.searchBar,
            onHide: context2.data.onHide
          };
        }
      },
      render: function render(context2) {
        if (context2.data.type === "contextmenu") {
          return {
            component: Menu,
            props: {
              items: context2.data.items,
              delay,
              searchBar: context2.data.searchBar,
              onHide: context2.data.onHide
            }
          };
        }
      }
    };
  }
  var index$3 = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    setup: setup$2
  });
  function px(value) {
    return "".concat(value, "px");
  }
  var _templateObject$6;
  var _templateObject2$6;
  function _createSuper$7(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$7();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$7() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$6(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$6();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$6(r5.d.map(_createElementDescriptor$6)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$6() {
    _getDecoratorsApi$6 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$6(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$6(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$6(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$6(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$6(def) {
    var key = _toPropertyKey$6(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$6(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$6(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$6(element.descriptor) || _isDataDescriptor$6(other.descriptor)) {
          if (_hasDecorators$6(element) || _hasDecorators$6(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$6(element)) {
            if (_hasDecorators$6(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$6(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$6(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$6(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$6(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$6(arg) {
    var key = _toPrimitive$6(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$6(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var MiniNode = _decorate$6(null, function(_initialize, _LitElement) {
    var MiniNode2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(MiniNode3, _LitElement2);
      var _super = _createSuper$7(MiniNode3);
      function MiniNode3() {
        var _this;
        _classCallCheck(this, MiniNode3);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(MiniNode3);
    })(_LitElement);
    return {
      F: MiniNode2,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "left",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "top",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "width",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "height",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject$6 || (_templateObject$6 = _taggedTemplateLiteral(["\n    .mini-node {\n      position: absolute;\n      background: rgba(110, 136, 255, 0.8);\n      border: 1px solid rgb(192 206 212 / 60%);\n    }\n  "])));
        }
      }, {
        kind: "get",
        key: "styles",
        value: function styles() {
          return {
            left: px(this.left),
            top: px(this.top),
            width: px(this.width),
            height: px(this.height)
          };
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          return b2(_templateObject2$6 || (_templateObject2$6 = _taggedTemplateLiteral(['\n      <div class="mini-node" style=', "></div>\n    "])), o6(this.styles));
        }
      }]
    };
  }, i4);
  customElements.define("mini-node", MiniNode);
  function ownKeys$42(object, enumerableOnly) {
    var keys2 = Object.keys(object);
    if (Object.getOwnPropertySymbols) {
      var symbols = Object.getOwnPropertySymbols(object);
      enumerableOnly && (symbols = symbols.filter(function(sym) {
        return Object.getOwnPropertyDescriptor(object, sym).enumerable;
      })), keys2.push.apply(keys2, symbols);
    }
    return keys2;
  }
  function _objectSpread$42(target) {
    for (var i7 = 1; i7 < arguments.length; i7++) {
      var source = null != arguments[i7] ? arguments[i7] : {};
      i7 % 2 ? ownKeys$42(Object(source), true).forEach(function(key) {
        _defineProperty(target, key, source[key]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(target, Object.getOwnPropertyDescriptors(source)) : ownKeys$42(Object(source)).forEach(function(key) {
        Object.defineProperty(target, key, Object.getOwnPropertyDescriptor(source, key));
      });
    }
    return target;
  }
  function useDrag(translate, getPointer) {
    return {
      start: function start(e7) {
        var previous = _objectSpread$42({}, getPointer(e7));
        function move(moveEvent) {
          var current = _objectSpread$42({}, getPointer(moveEvent));
          var dx = current.x - previous.x;
          var dy = current.y - previous.y;
          previous = current;
          translate(dx, dy);
        }
        function up() {
          window.removeEventListener("pointermove", move);
          window.removeEventListener("pointerup", up);
          window.removeEventListener("pointercancel", up);
        }
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", up);
        window.addEventListener("pointercancel", up);
      }
    };
  }
  var _templateObject$5;
  var _templateObject2$5;
  function _createSuper$6(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$6();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$6() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$5(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$5();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$5(r5.d.map(_createElementDescriptor$5)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$5() {
    _getDecoratorsApi$5 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$5(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$5(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$5(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$5(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$5(def) {
    var key = _toPropertyKey$5(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$5(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$5(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$5(element.descriptor) || _isDataDescriptor$5(other.descriptor)) {
          if (_hasDecorators$5(element) || _hasDecorators$5(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$5(element)) {
            if (_hasDecorators$5(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$5(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$5(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$5(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$5(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$5(arg) {
    var key = _toPrimitive$5(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$5(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var MiniViewport = _decorate$5(null, function(_initialize, _LitElement) {
    var MiniViewport2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(MiniViewport3, _LitElement2);
      var _super = _createSuper$6(MiniViewport3);
      function MiniViewport3() {
        var _this;
        _classCallCheck(this, MiniViewport3);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(MiniViewport3);
    })(_LitElement);
    return {
      F: MiniViewport2,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "left",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "top",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "width",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "height",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "containerWidth",
        value: function value() {
          return 0;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "reTranslate",
        value: function value() {
          return function(_x, _y) {
            throw new Error("TRANSLATE NOT SET PROPERLY IN MiniViewport");
          };
        }
      }, {
        kind: "field",
        key: "drag",
        value: function value() {
          return useDrag(this.onDrag, function(e7) {
            return {
              x: e7.pageX,
              y: e7.pageY
            };
          });
        }
      }, {
        kind: "method",
        key: "scale",
        value: (
          // eslint-disable-next-line @typescript-eslint/naming-convention
          function scale(v2) {
            return v2 * this.containerWidth;
          }
        )
      }, {
        kind: "method",
        key: "invert",
        value: function invert(v2) {
          return v2 / this.containerWidth;
        }
      }, {
        kind: "method",
        key: "onDrag",
        value: function onDrag(dx, dy) {
          this.reTranslate(this.invert(-dx), this.invert(-dy));
        }
      }, {
        kind: "get",
        key: "styles",
        value: function styles() {
          return {
            left: px(this.scale(this.left)),
            top: px(this.scale(this.top)),
            width: px(this.scale(this.width)),
            height: px(this.scale(this.height))
          };
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          return b2(_templateObject$5 || (_templateObject$5 = _taggedTemplateLiteral(['\n      <div\n        class="mini-viewport"\n        @pointerdown=', "\n        style=", '\n        data-testid="minimap-viewport"\n      ></div>\n    '])), this.drag.start, this.styles);
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject2$5 || (_templateObject2$5 = _taggedTemplateLiteral(["\n    .mini-viewport {\n      position: absolute;\n      background: rgba(255, 251, 128, 0.32);\n      border: 1px solid #ffe52b;\n    }\n  "])));
        }
      }]
    };
  }, i4);
  customElements.define("mini-viewport", MiniViewport);
  var getOwnPropertyNames = Object.getOwnPropertyNames;
  var getOwnPropertySymbols = Object.getOwnPropertySymbols;
  var hasOwnProperty = Object.prototype.hasOwnProperty;
  function combineComparators(comparatorA, comparatorB) {
    return function isEqual(a3, b3, state) {
      return comparatorA(a3, b3, state) && comparatorB(a3, b3, state);
    };
  }
  function createIsCircular(areItemsEqual) {
    return function isCircular(a3, b3, state) {
      if (!a3 || !b3 || typeof a3 !== "object" || typeof b3 !== "object") {
        return areItemsEqual(a3, b3, state);
      }
      var cache = state.cache;
      var cachedA = cache.get(a3);
      var cachedB = cache.get(b3);
      if (cachedA && cachedB) {
        return cachedA === b3 && cachedB === a3;
      }
      cache.set(a3, b3);
      cache.set(b3, a3);
      var result = areItemsEqual(a3, b3, state);
      cache.delete(a3);
      cache.delete(b3);
      return result;
    };
  }
  function getStrictProperties(object) {
    return getOwnPropertyNames(object).concat(getOwnPropertySymbols(object));
  }
  var hasOwn = Object.hasOwn || (function(object, property) {
    return hasOwnProperty.call(object, property);
  });
  function sameValueZeroEqual(a3, b3) {
    return a3 || b3 ? a3 === b3 : a3 === b3 || a3 !== a3 && b3 !== b3;
  }
  var OWNER = "_owner";
  var getOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;
  var keys = Object.keys;
  function areArraysEqual(a3, b3, state) {
    var index4 = a3.length;
    if (b3.length !== index4) {
      return false;
    }
    while (index4-- > 0) {
      if (!state.equals(a3[index4], b3[index4], index4, index4, a3, b3, state)) {
        return false;
      }
    }
    return true;
  }
  function areDatesEqual(a3, b3) {
    return sameValueZeroEqual(a3.getTime(), b3.getTime());
  }
  function areMapsEqual(a3, b3, state) {
    if (a3.size !== b3.size) {
      return false;
    }
    var matchedIndices = {};
    var aIterable = a3.entries();
    var index4 = 0;
    var aResult;
    var bResult;
    while (aResult = aIterable.next()) {
      if (aResult.done) {
        break;
      }
      var bIterable = b3.entries();
      var hasMatch = false;
      var matchIndex = 0;
      while (bResult = bIterable.next()) {
        if (bResult.done) {
          break;
        }
        var _a = aResult.value, aKey = _a[0], aValue = _a[1];
        var _b = bResult.value, bKey = _b[0], bValue = _b[1];
        if (!hasMatch && !matchedIndices[matchIndex] && (hasMatch = state.equals(aKey, bKey, index4, matchIndex, a3, b3, state) && state.equals(aValue, bValue, aKey, bKey, a3, b3, state))) {
          matchedIndices[matchIndex] = true;
        }
        matchIndex++;
      }
      if (!hasMatch) {
        return false;
      }
      index4++;
    }
    return true;
  }
  function areObjectsEqual(a3, b3, state) {
    var properties = keys(a3);
    var index4 = properties.length;
    if (keys(b3).length !== index4) {
      return false;
    }
    var property;
    while (index4-- > 0) {
      property = properties[index4];
      if (property === OWNER && (a3.$$typeof || b3.$$typeof) && a3.$$typeof !== b3.$$typeof) {
        return false;
      }
      if (!hasOwn(b3, property) || !state.equals(a3[property], b3[property], property, property, a3, b3, state)) {
        return false;
      }
    }
    return true;
  }
  function areObjectsEqualStrict(a3, b3, state) {
    var properties = getStrictProperties(a3);
    var index4 = properties.length;
    if (getStrictProperties(b3).length !== index4) {
      return false;
    }
    var property;
    var descriptorA;
    var descriptorB;
    while (index4-- > 0) {
      property = properties[index4];
      if (property === OWNER && (a3.$$typeof || b3.$$typeof) && a3.$$typeof !== b3.$$typeof) {
        return false;
      }
      if (!hasOwn(b3, property)) {
        return false;
      }
      if (!state.equals(a3[property], b3[property], property, property, a3, b3, state)) {
        return false;
      }
      descriptorA = getOwnPropertyDescriptor(a3, property);
      descriptorB = getOwnPropertyDescriptor(b3, property);
      if ((descriptorA || descriptorB) && (!descriptorA || !descriptorB || descriptorA.configurable !== descriptorB.configurable || descriptorA.enumerable !== descriptorB.enumerable || descriptorA.writable !== descriptorB.writable)) {
        return false;
      }
    }
    return true;
  }
  function arePrimitiveWrappersEqual(a3, b3) {
    return sameValueZeroEqual(a3.valueOf(), b3.valueOf());
  }
  function areRegExpsEqual(a3, b3) {
    return a3.source === b3.source && a3.flags === b3.flags;
  }
  function areSetsEqual(a3, b3, state) {
    if (a3.size !== b3.size) {
      return false;
    }
    var matchedIndices = {};
    var aIterable = a3.values();
    var aResult;
    var bResult;
    while (aResult = aIterable.next()) {
      if (aResult.done) {
        break;
      }
      var bIterable = b3.values();
      var hasMatch = false;
      var matchIndex = 0;
      while (bResult = bIterable.next()) {
        if (bResult.done) {
          break;
        }
        if (!hasMatch && !matchedIndices[matchIndex] && (hasMatch = state.equals(aResult.value, bResult.value, aResult.value, bResult.value, a3, b3, state))) {
          matchedIndices[matchIndex] = true;
        }
        matchIndex++;
      }
      if (!hasMatch) {
        return false;
      }
    }
    return true;
  }
  function areTypedArraysEqual(a3, b3) {
    var index4 = a3.length;
    if (b3.length !== index4) {
      return false;
    }
    while (index4-- > 0) {
      if (a3[index4] !== b3[index4]) {
        return false;
      }
    }
    return true;
  }
  var ARGUMENTS_TAG = "[object Arguments]";
  var BOOLEAN_TAG = "[object Boolean]";
  var DATE_TAG = "[object Date]";
  var MAP_TAG = "[object Map]";
  var NUMBER_TAG = "[object Number]";
  var OBJECT_TAG = "[object Object]";
  var REG_EXP_TAG = "[object RegExp]";
  var SET_TAG = "[object Set]";
  var STRING_TAG = "[object String]";
  var isArray = Array.isArray;
  var isTypedArray = typeof ArrayBuffer === "function" && ArrayBuffer.isView ? ArrayBuffer.isView : null;
  var assign = Object.assign;
  var getTag = Object.prototype.toString.call.bind(Object.prototype.toString);
  function createEqualityComparator(_a) {
    var areArraysEqual2 = _a.areArraysEqual, areDatesEqual2 = _a.areDatesEqual, areMapsEqual2 = _a.areMapsEqual, areObjectsEqual2 = _a.areObjectsEqual, arePrimitiveWrappersEqual2 = _a.arePrimitiveWrappersEqual, areRegExpsEqual2 = _a.areRegExpsEqual, areSetsEqual2 = _a.areSetsEqual, areTypedArraysEqual2 = _a.areTypedArraysEqual;
    return function comparator(a3, b3, state) {
      if (a3 === b3) {
        return true;
      }
      if (a3 == null || b3 == null || typeof a3 !== "object" || typeof b3 !== "object") {
        return a3 !== a3 && b3 !== b3;
      }
      var constructor = a3.constructor;
      if (constructor !== b3.constructor) {
        return false;
      }
      if (constructor === Object) {
        return areObjectsEqual2(a3, b3, state);
      }
      if (isArray(a3)) {
        return areArraysEqual2(a3, b3, state);
      }
      if (isTypedArray != null && isTypedArray(a3)) {
        return areTypedArraysEqual2(a3, b3, state);
      }
      if (constructor === Date) {
        return areDatesEqual2(a3, b3, state);
      }
      if (constructor === RegExp) {
        return areRegExpsEqual2(a3, b3, state);
      }
      if (constructor === Map) {
        return areMapsEqual2(a3, b3, state);
      }
      if (constructor === Set) {
        return areSetsEqual2(a3, b3, state);
      }
      var tag = getTag(a3);
      if (tag === DATE_TAG) {
        return areDatesEqual2(a3, b3, state);
      }
      if (tag === REG_EXP_TAG) {
        return areRegExpsEqual2(a3, b3, state);
      }
      if (tag === MAP_TAG) {
        return areMapsEqual2(a3, b3, state);
      }
      if (tag === SET_TAG) {
        return areSetsEqual2(a3, b3, state);
      }
      if (tag === OBJECT_TAG) {
        return typeof a3.then !== "function" && typeof b3.then !== "function" && areObjectsEqual2(a3, b3, state);
      }
      if (tag === ARGUMENTS_TAG) {
        return areObjectsEqual2(a3, b3, state);
      }
      if (tag === BOOLEAN_TAG || tag === NUMBER_TAG || tag === STRING_TAG) {
        return arePrimitiveWrappersEqual2(a3, b3, state);
      }
      return false;
    };
  }
  function createEqualityComparatorConfig(_a) {
    var circular = _a.circular, createCustomConfig = _a.createCustomConfig, strict = _a.strict;
    var config2 = {
      areArraysEqual: strict ? areObjectsEqualStrict : areArraysEqual,
      areDatesEqual,
      areMapsEqual: strict ? combineComparators(areMapsEqual, areObjectsEqualStrict) : areMapsEqual,
      areObjectsEqual: strict ? areObjectsEqualStrict : areObjectsEqual,
      arePrimitiveWrappersEqual,
      areRegExpsEqual,
      areSetsEqual: strict ? combineComparators(areSetsEqual, areObjectsEqualStrict) : areSetsEqual,
      areTypedArraysEqual: strict ? areObjectsEqualStrict : areTypedArraysEqual
    };
    if (createCustomConfig) {
      config2 = assign({}, config2, createCustomConfig(config2));
    }
    if (circular) {
      var areArraysEqual$1 = createIsCircular(config2.areArraysEqual);
      var areMapsEqual$1 = createIsCircular(config2.areMapsEqual);
      var areObjectsEqual$1 = createIsCircular(config2.areObjectsEqual);
      var areSetsEqual$1 = createIsCircular(config2.areSetsEqual);
      config2 = assign({}, config2, {
        areArraysEqual: areArraysEqual$1,
        areMapsEqual: areMapsEqual$1,
        areObjectsEqual: areObjectsEqual$1,
        areSetsEqual: areSetsEqual$1
      });
    }
    return config2;
  }
  function createInternalEqualityComparator(compare) {
    return function(a3, b3, _indexOrKeyA, _indexOrKeyB, _parentA, _parentB, state) {
      return compare(a3, b3, state);
    };
  }
  function createIsEqual(_a) {
    var circular = _a.circular, comparator = _a.comparator, createState = _a.createState, equals = _a.equals, strict = _a.strict;
    if (createState) {
      return function isEqual(a3, b3) {
        var _a2 = createState(), _b = _a2.cache, cache = _b === void 0 ? circular ? /* @__PURE__ */ new WeakMap() : void 0 : _b, meta = _a2.meta;
        return comparator(a3, b3, {
          cache,
          equals,
          meta,
          strict
        });
      };
    }
    if (circular) {
      return function isEqual(a3, b3) {
        return comparator(a3, b3, {
          cache: /* @__PURE__ */ new WeakMap(),
          equals,
          meta: void 0,
          strict
        });
      };
    }
    var state = {
      cache: void 0,
      equals,
      meta: void 0,
      strict
    };
    return function isEqual(a3, b3) {
      return comparator(a3, b3, state);
    };
  }
  var deepEqual = createCustomEqual();
  createCustomEqual({ strict: true });
  createCustomEqual({ circular: true });
  createCustomEqual({
    circular: true,
    strict: true
  });
  createCustomEqual({
    createInternalComparator: function() {
      return sameValueZeroEqual;
    }
  });
  createCustomEqual({
    strict: true,
    createInternalComparator: function() {
      return sameValueZeroEqual;
    }
  });
  createCustomEqual({
    circular: true,
    createInternalComparator: function() {
      return sameValueZeroEqual;
    }
  });
  createCustomEqual({
    circular: true,
    createInternalComparator: function() {
      return sameValueZeroEqual;
    },
    strict: true
  });
  function createCustomEqual(options) {
    if (options === void 0) {
      options = {};
    }
    var _a = options.circular, circular = _a === void 0 ? false : _a, createCustomInternalComparator = options.createInternalComparator, createState = options.createState, _b = options.strict, strict = _b === void 0 ? false : _b;
    var config2 = createEqualityComparatorConfig(options);
    var comparator = createEqualityComparator(config2);
    var equals = createCustomInternalComparator ? createCustomInternalComparator(comparator) : createInternalEqualityComparator(comparator);
    return createIsEqual({ circular, comparator, createState, equals, strict });
  }
  var _templateObject$4;
  var _templateObject2$4;
  var _templateObject3$2;
  function _createSuper$5(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$5();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$5() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$4(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$4();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$4(r5.d.map(_createElementDescriptor$4)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$4() {
    _getDecoratorsApi$4 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$4(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$4(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$4(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$4(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$4(def) {
    var key = _toPropertyKey$4(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$4(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$4(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$4(element.descriptor) || _isDataDescriptor$4(other.descriptor)) {
          if (_hasDecorators$4(element) || _hasDecorators$4(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$4(element)) {
            if (_hasDecorators$4(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$4(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$4(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$4(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$4(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$4(arg) {
    var key = _toPrimitive$4(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$4(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var Minimap = _decorate$4([t3("minimap-component")], function(_initialize, _LitElement) {
    var Minimap2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Minimap3, _LitElement2);
      var _super = _createSuper$5(Minimap3);
      function Minimap3() {
        var _this;
        _classCallCheck(this, Minimap3);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Minimap3);
    })(_LitElement);
    return {
      F: Minimap2,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "size",
        value: function value() {
          return 200;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "ratio",
        value: function value() {
          return 1;
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Array,
          hasChanged: function hasChanged(newVal, oldVal) {
            return !deepEqual(newVal, oldVal);
          }
        })],
        key: "nodes",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "viewport",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "reTranslate",
        value: function value() {
          return function() {
            throw new Error("reTranslate not set properly");
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "point",
        value: function value() {
          return function(_x, _y) {
            throw new Error("point not defined Properly");
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Number
        })],
        key: "seed",
        value: void 0
      }, {
        kind: "field",
        decorators: [e5(".minimap")],
        key: "container",
        value: void 0
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject$4 || (_templateObject$4 = _taggedTemplateLiteral(["\n        .minimap {\n            position: absolute;\n            right: 24px;\n            bottom: 24px;\n            background: rgba(229, 234, 239, 0.65);\n            padding: 20px;\n            overflow: hidden;\n            border: 1px solid #b1b7ff;\n            border-radius: 8px;\n            box-sizing: border-box;\n        }\n    "])));
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          var _this$nodes, _this2 = this;
          return b2(_templateObject2$4 || (_templateObject2$4 = _taggedTemplateLiteral(['\n            <div\n                class="minimap"\n                style="width: ', "; height: ", '"\n                @pointerdown="', '"\n                @dblclick="', '"\n                data-testid="minimap"\n            >\n                ', '\n                <mini-viewport\n                    .left="', '"\n                    .top="', '"\n                    .width="', '"\n                    .height="', '"\n                    .containerWidth="', '"\n                    .reTranslate="', '"\n                ></mini-viewport>\n            </div>\n        '])), px(this.size * this.ratio), px(this.size), this.stopEvent, this.dblclick, (_this$nodes = this.nodes) === null || _this$nodes === void 0 ? void 0 : _this$nodes.map(function(node) {
            return b2(_templateObject3$2 || (_templateObject3$2 = _taggedTemplateLiteral(['\n                        <mini-node\n                            .left="', '"\n                            .top="', '"\n                            .width="', '"\n                            .height="', '"\n                        ></mini-node>\n                    '])), _this2.scale(node.left), _this2.scale(node.top), _this2.scale(node.width), _this2.scale(node.height));
          }), this.viewport.left, this.viewport.top, this.viewport.width, this.viewport.height, this.container && this.container.clientWidth, this.reTranslate);
        }
      }, {
        kind: "method",
        key: "stopEvent",
        value: function stopEvent(e7) {
          e7.stopPropagation();
          e7.preventDefault();
        }
      }, {
        kind: "method",
        key: "dblclick",
        value: function dblclick(e7) {
          var _this$point;
          if (!this.container) return;
          var box = this.container.getBoundingClientRect();
          var x2 = (e7.clientX - box.left) / (this.size * this.ratio);
          var y3 = (e7.clientY - box.top) / (this.size * this.ratio);
          (_this$point = this.point) === null || _this$point === void 0 ? void 0 : _this$point.call(this, x2, y3);
        }
      }, {
        kind: "method",
        key: "scale",
        value: function scale(value) {
          if (!this.container) return 0;
          return value * this.container.clientWidth;
        }
      }]
    };
  }, i4);
  function setup$1(props) {
    return {
      update: function update(context2) {
        if (context2.data.type === "minimap") {
          return {
            nodes: context2.data.nodes,
            size: (props === null || props === void 0 ? void 0 : props.size) || 200,
            ratio: context2.data.ratio,
            viewport: context2.data.viewport,
            reTranslate: context2.data.translate,
            point: context2.data.point
          };
        }
      },
      render: function render(context2) {
        if (context2.data.type === "minimap") {
          return {
            component: Minimap,
            props: {
              nodes: context2.data.nodes,
              size: (props === null || props === void 0 ? void 0 : props.size) || 200,
              ratio: context2.data.ratio,
              viewport: context2.data.viewport,
              reTranslate: context2.data.translate,
              point: context2.data.point
            }
          };
        }
      }
    };
  }
  var index$2 = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    setup: setup$1
  });
  var _templateObject$3;
  var _templateObject2$3;
  var _templateObject3$1;
  function _createSuper$4(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$4();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$4() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$3(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$3();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$3(r5.d.map(_createElementDescriptor$3)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$3() {
    _getDecoratorsApi$3 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$3(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$3(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$3(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$3(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$3(def) {
    var key = _toPropertyKey$3(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$3(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$3(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$3(element.descriptor) || _isDataDescriptor$3(other.descriptor)) {
          if (_hasDecorators$3(element) || _hasDecorators$3(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$3(element)) {
            if (_hasDecorators$3(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$3(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$3(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$3(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$3(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$3(arg) {
    var key = _toPrimitive$3(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$3(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var Pins = _decorate$3([t3("pins-component")], function(_initialize, _LitElement) {
    var Pins2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Pins3, _LitElement2);
      var _super = _createSuper$4(Pins3);
      function Pins3() {
        var _this;
        _classCallCheck(this, Pins3);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(Pins3);
    })(_LitElement);
    return {
      F: Pins2,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Array
        })],
        key: "pins",
        value: function value() {
          return [];
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "menu",
        value: function value() {
          return function(_id) {
            throw new Error("menu not set properly");
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "reTranslate",
        value: function value() {
          return function(_id, _dx, _dy) {
            throw new Error("reTranslate not set properly");
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "down",
        value: function value() {
          return function(_id) {
            throw new Error("down not set properly");
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "getPointer",
        value: function value() {
          return function() {
            throw new Error("getPointer not set properly");
          };
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject$3 || (_templateObject$3 = _taggedTemplateLiteral(["\n    :host {\n      display: block;\n    }\n  "])));
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          var _this2 = this;
          return b2(_templateObject2$3 || (_templateObject2$3 = _taggedTemplateLiteral(["\n      ", "\n    "])), this.pins.map(function(pin) {
            return b2(_templateObject3$1 || (_templateObject3$1 = _taggedTemplateLiteral(["\n          <pin-component\n            .position=", "\n            .selected=", "\n            .getPointer=", "\n            @menu=", "\n            @translate=", "\n            @down=", "\n          ></pin-component>\n        "])), pin.position, pin.selected, _this2.getPointer, function() {
              return _this2.menu(pin.id);
            }, function(e7) {
              return _this2.reTranslate(pin.id, e7.detail.dx, e7.detail.dy);
            }, function() {
              return _this2.down(pin.id);
            });
          }));
        }
      }]
    };
  }, i4);
  function ownKeys$32(object, enumerableOnly) {
    var keys2 = Object.keys(object);
    if (Object.getOwnPropertySymbols) {
      var symbols = Object.getOwnPropertySymbols(object);
      enumerableOnly && (symbols = symbols.filter(function(sym) {
        return Object.getOwnPropertyDescriptor(object, sym).enumerable;
      })), keys2.push.apply(keys2, symbols);
    }
    return keys2;
  }
  function _objectSpread$32(target) {
    for (var i7 = 1; i7 < arguments.length; i7++) {
      var source = null != arguments[i7] ? arguments[i7] : {};
      i7 % 2 ? ownKeys$32(Object(source), true).forEach(function(key) {
        _defineProperty(target, key, source[key]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(target, Object.getOwnPropertyDescriptors(source)) : ownKeys$32(Object(source)).forEach(function(key) {
        Object.defineProperty(target, key, Object.getOwnPropertyDescriptor(source, key));
      });
    }
    return target;
  }
  function setup2(props) {
    var getProps = function getProps2() {
      return {
        menu: (props === null || props === void 0 ? void 0 : props.contextMenu) || function() {
          return null;
        },
        translate: (props === null || props === void 0 ? void 0 : props.translate) || function() {
          return null;
        },
        down: (props === null || props === void 0 ? void 0 : props.pointerdown) || function() {
          return null;
        }
      };
    };
    return {
      update: function update(context2) {
        if (context2.data.type === "reroute-pins") {
          return _objectSpread$32(_objectSpread$32({}, getProps()), {}, {
            pins: context2.data.data.pins
          });
        }
      },
      render: function render(context2, plugin) {
        if (context2.data.type === "reroute-pins") {
          var area = plugin.parentScope(BaseAreaPlugin);
          return {
            component: Pins,
            props: _objectSpread$32(_objectSpread$32({}, getProps()), {}, {
              getPointer: function getPointer() {
                return area.area.pointer;
              },
              pins: context2.data.data.pins
            })
          };
        }
      }
    };
  }
  var index$1 = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    setup: setup2
  });
  var index3 = /* @__PURE__ */ Object.freeze({
    __proto__: null,
    classic: index$4,
    contextMenu: index$3,
    minimap: index$2,
    reroute: index$1
  });
  var extendStatics = function(d3, b3) {
    extendStatics = Object.setPrototypeOf || { __proto__: [] } instanceof Array && function(d4, b4) {
      d4.__proto__ = b4;
    } || function(d4, b4) {
      for (var p3 in b4) if (Object.prototype.hasOwnProperty.call(b4, p3)) d4[p3] = b4[p3];
    };
    return extendStatics(d3, b3);
  };
  function __extends(d3, b3) {
    if (typeof b3 !== "function" && b3 !== null)
      throw new TypeError("Class extends value " + String(b3) + " is not a constructor or null");
    extendStatics(d3, b3);
    function __() {
      this.constructor = d3;
    }
    d3.prototype = b3 === null ? Object.create(b3) : (__.prototype = b3.prototype, new __());
  }
  function __values(o7) {
    var s4 = typeof Symbol === "function" && Symbol.iterator, m2 = s4 && o7[s4], i7 = 0;
    if (m2) return m2.call(o7);
    if (o7 && typeof o7.length === "number") return {
      next: function() {
        if (o7 && i7 >= o7.length) o7 = void 0;
        return { value: o7 && o7[i7++], done: !o7 };
      }
    };
    throw new TypeError(s4 ? "Object is not iterable." : "Symbol.iterator is not defined.");
  }
  function __read(o7, n6) {
    var m2 = typeof Symbol === "function" && o7[Symbol.iterator];
    if (!m2) return o7;
    var i7 = m2.call(o7), r5, ar = [], e7;
    try {
      while ((n6 === void 0 || n6-- > 0) && !(r5 = i7.next()).done) ar.push(r5.value);
    } catch (error) {
      e7 = { error };
    } finally {
      try {
        if (r5 && !r5.done && (m2 = i7["return"])) m2.call(i7);
      } finally {
        if (e7) throw e7.error;
      }
    }
    return ar;
  }
  function __spreadArray(to, from, pack) {
    if (pack || arguments.length === 2) for (var i7 = 0, l3 = from.length, ar; i7 < l3; i7++) {
      if (ar || !(i7 in from)) {
        if (!ar) ar = Array.prototype.slice.call(from, 0, i7);
        ar[i7] = from[i7];
      }
    }
    return to.concat(ar || Array.prototype.slice.call(from));
  }
  function isFunction(value) {
    return typeof value === "function";
  }
  function createErrorClass(createImpl) {
    var _super = function(instance) {
      Error.call(instance);
      instance.stack = new Error().stack;
    };
    var ctorFunc = createImpl(_super);
    ctorFunc.prototype = Object.create(Error.prototype);
    ctorFunc.prototype.constructor = ctorFunc;
    return ctorFunc;
  }
  var UnsubscriptionError = createErrorClass(function(_super) {
    return function UnsubscriptionErrorImpl(errors) {
      _super(this);
      this.message = errors ? errors.length + " errors occurred during unsubscription:\n" + errors.map(function(err, i7) {
        return i7 + 1 + ") " + err.toString();
      }).join("\n  ") : "";
      this.name = "UnsubscriptionError";
      this.errors = errors;
    };
  });
  function arrRemove(arr, item) {
    if (arr) {
      var index4 = arr.indexOf(item);
      0 <= index4 && arr.splice(index4, 1);
    }
  }
  var Subscription = (function() {
    function Subscription2(initialTeardown) {
      this.initialTeardown = initialTeardown;
      this.closed = false;
      this._parentage = null;
      this._finalizers = null;
    }
    Subscription2.prototype.unsubscribe = function() {
      var e_1, _a, e_2, _b;
      var errors;
      if (!this.closed) {
        this.closed = true;
        var _parentage = this._parentage;
        if (_parentage) {
          this._parentage = null;
          if (Array.isArray(_parentage)) {
            try {
              for (var _parentage_1 = __values(_parentage), _parentage_1_1 = _parentage_1.next(); !_parentage_1_1.done; _parentage_1_1 = _parentage_1.next()) {
                var parent_1 = _parentage_1_1.value;
                parent_1.remove(this);
              }
            } catch (e_1_1) {
              e_1 = { error: e_1_1 };
            } finally {
              try {
                if (_parentage_1_1 && !_parentage_1_1.done && (_a = _parentage_1.return)) _a.call(_parentage_1);
              } finally {
                if (e_1) throw e_1.error;
              }
            }
          } else {
            _parentage.remove(this);
          }
        }
        var initialFinalizer = this.initialTeardown;
        if (isFunction(initialFinalizer)) {
          try {
            initialFinalizer();
          } catch (e7) {
            errors = e7 instanceof UnsubscriptionError ? e7.errors : [e7];
          }
        }
        var _finalizers = this._finalizers;
        if (_finalizers) {
          this._finalizers = null;
          try {
            for (var _finalizers_1 = __values(_finalizers), _finalizers_1_1 = _finalizers_1.next(); !_finalizers_1_1.done; _finalizers_1_1 = _finalizers_1.next()) {
              var finalizer = _finalizers_1_1.value;
              try {
                execFinalizer(finalizer);
              } catch (err) {
                errors = errors !== null && errors !== void 0 ? errors : [];
                if (err instanceof UnsubscriptionError) {
                  errors = __spreadArray(__spreadArray([], __read(errors)), __read(err.errors));
                } else {
                  errors.push(err);
                }
              }
            }
          } catch (e_2_1) {
            e_2 = { error: e_2_1 };
          } finally {
            try {
              if (_finalizers_1_1 && !_finalizers_1_1.done && (_b = _finalizers_1.return)) _b.call(_finalizers_1);
            } finally {
              if (e_2) throw e_2.error;
            }
          }
        }
        if (errors) {
          throw new UnsubscriptionError(errors);
        }
      }
    };
    Subscription2.prototype.add = function(teardown) {
      var _a;
      if (teardown && teardown !== this) {
        if (this.closed) {
          execFinalizer(teardown);
        } else {
          if (teardown instanceof Subscription2) {
            if (teardown.closed || teardown._hasParent(this)) {
              return;
            }
            teardown._addParent(this);
          }
          (this._finalizers = (_a = this._finalizers) !== null && _a !== void 0 ? _a : []).push(teardown);
        }
      }
    };
    Subscription2.prototype._hasParent = function(parent) {
      var _parentage = this._parentage;
      return _parentage === parent || Array.isArray(_parentage) && _parentage.includes(parent);
    };
    Subscription2.prototype._addParent = function(parent) {
      var _parentage = this._parentage;
      this._parentage = Array.isArray(_parentage) ? (_parentage.push(parent), _parentage) : _parentage ? [_parentage, parent] : parent;
    };
    Subscription2.prototype._removeParent = function(parent) {
      var _parentage = this._parentage;
      if (_parentage === parent) {
        this._parentage = null;
      } else if (Array.isArray(_parentage)) {
        arrRemove(_parentage, parent);
      }
    };
    Subscription2.prototype.remove = function(teardown) {
      var _finalizers = this._finalizers;
      _finalizers && arrRemove(_finalizers, teardown);
      if (teardown instanceof Subscription2) {
        teardown._removeParent(this);
      }
    };
    Subscription2.EMPTY = (function() {
      var empty = new Subscription2();
      empty.closed = true;
      return empty;
    })();
    return Subscription2;
  })();
  var EMPTY_SUBSCRIPTION = Subscription.EMPTY;
  function isSubscription(value) {
    return value instanceof Subscription || value && "closed" in value && isFunction(value.remove) && isFunction(value.add) && isFunction(value.unsubscribe);
  }
  function execFinalizer(finalizer) {
    if (isFunction(finalizer)) {
      finalizer();
    } else {
      finalizer.unsubscribe();
    }
  }
  var config = {
    onUnhandledError: null,
    onStoppedNotification: null,
    Promise: void 0,
    useDeprecatedSynchronousErrorHandling: false,
    useDeprecatedNextContext: false
  };
  var timeoutProvider = {
    setTimeout: function(handler, timeout) {
      var args = [];
      for (var _i = 2; _i < arguments.length; _i++) {
        args[_i - 2] = arguments[_i];
      }
      var delegate = timeoutProvider.delegate;
      if (delegate === null || delegate === void 0 ? void 0 : delegate.setTimeout) {
        return delegate.setTimeout.apply(delegate, __spreadArray([handler, timeout], __read(args)));
      }
      return setTimeout.apply(void 0, __spreadArray([handler, timeout], __read(args)));
    },
    clearTimeout: function(handle) {
      var delegate = timeoutProvider.delegate;
      return ((delegate === null || delegate === void 0 ? void 0 : delegate.clearTimeout) || clearTimeout)(handle);
    },
    delegate: void 0
  };
  function reportUnhandledError(err) {
    timeoutProvider.setTimeout(function() {
      {
        throw err;
      }
    });
  }
  function noop() {
  }
  var context = null;
  function errorContext(cb) {
    if (config.useDeprecatedSynchronousErrorHandling) {
      var isRoot = !context;
      if (isRoot) {
        context = { errorThrown: false, error: null };
      }
      cb();
      if (isRoot) {
        var _a = context, errorThrown = _a.errorThrown, error = _a.error;
        context = null;
        if (errorThrown) {
          throw error;
        }
      }
    } else {
      cb();
    }
  }
  var Subscriber = (function(_super) {
    __extends(Subscriber2, _super);
    function Subscriber2(destination) {
      var _this = _super.call(this) || this;
      _this.isStopped = false;
      if (destination) {
        _this.destination = destination;
        if (isSubscription(destination)) {
          destination.add(_this);
        }
      } else {
        _this.destination = EMPTY_OBSERVER;
      }
      return _this;
    }
    Subscriber2.create = function(next, error, complete) {
      return new SafeSubscriber(next, error, complete);
    };
    Subscriber2.prototype.next = function(value) {
      if (this.isStopped) ;
      else {
        this._next(value);
      }
    };
    Subscriber2.prototype.error = function(err) {
      if (this.isStopped) ;
      else {
        this.isStopped = true;
        this._error(err);
      }
    };
    Subscriber2.prototype.complete = function() {
      if (this.isStopped) ;
      else {
        this.isStopped = true;
        this._complete();
      }
    };
    Subscriber2.prototype.unsubscribe = function() {
      if (!this.closed) {
        this.isStopped = true;
        _super.prototype.unsubscribe.call(this);
        this.destination = null;
      }
    };
    Subscriber2.prototype._next = function(value) {
      this.destination.next(value);
    };
    Subscriber2.prototype._error = function(err) {
      try {
        this.destination.error(err);
      } finally {
        this.unsubscribe();
      }
    };
    Subscriber2.prototype._complete = function() {
      try {
        this.destination.complete();
      } finally {
        this.unsubscribe();
      }
    };
    return Subscriber2;
  })(Subscription);
  var _bind = Function.prototype.bind;
  function bind(fn, thisArg) {
    return _bind.call(fn, thisArg);
  }
  var ConsumerObserver = (function() {
    function ConsumerObserver2(partialObserver) {
      this.partialObserver = partialObserver;
    }
    ConsumerObserver2.prototype.next = function(value) {
      var partialObserver = this.partialObserver;
      if (partialObserver.next) {
        try {
          partialObserver.next(value);
        } catch (error) {
          handleUnhandledError(error);
        }
      }
    };
    ConsumerObserver2.prototype.error = function(err) {
      var partialObserver = this.partialObserver;
      if (partialObserver.error) {
        try {
          partialObserver.error(err);
        } catch (error) {
          handleUnhandledError(error);
        }
      } else {
        handleUnhandledError(err);
      }
    };
    ConsumerObserver2.prototype.complete = function() {
      var partialObserver = this.partialObserver;
      if (partialObserver.complete) {
        try {
          partialObserver.complete();
        } catch (error) {
          handleUnhandledError(error);
        }
      }
    };
    return ConsumerObserver2;
  })();
  var SafeSubscriber = (function(_super) {
    __extends(SafeSubscriber2, _super);
    function SafeSubscriber2(observerOrNext, error, complete) {
      var _this = _super.call(this) || this;
      var partialObserver;
      if (isFunction(observerOrNext) || !observerOrNext) {
        partialObserver = {
          next: observerOrNext !== null && observerOrNext !== void 0 ? observerOrNext : void 0,
          error: error !== null && error !== void 0 ? error : void 0,
          complete: complete !== null && complete !== void 0 ? complete : void 0
        };
      } else {
        var context_1;
        if (_this && config.useDeprecatedNextContext) {
          context_1 = Object.create(observerOrNext);
          context_1.unsubscribe = function() {
            return _this.unsubscribe();
          };
          partialObserver = {
            next: observerOrNext.next && bind(observerOrNext.next, context_1),
            error: observerOrNext.error && bind(observerOrNext.error, context_1),
            complete: observerOrNext.complete && bind(observerOrNext.complete, context_1)
          };
        } else {
          partialObserver = observerOrNext;
        }
      }
      _this.destination = new ConsumerObserver(partialObserver);
      return _this;
    }
    return SafeSubscriber2;
  })(Subscriber);
  function handleUnhandledError(error) {
    {
      reportUnhandledError(error);
    }
  }
  function defaultErrorHandler(err) {
    throw err;
  }
  var EMPTY_OBSERVER = {
    closed: true,
    next: noop,
    error: defaultErrorHandler,
    complete: noop
  };
  var observable = (function() {
    return typeof Symbol === "function" && Symbol.observable || "@@observable";
  })();
  function identity(x2) {
    return x2;
  }
  function pipeFromArray(fns) {
    if (fns.length === 0) {
      return identity;
    }
    if (fns.length === 1) {
      return fns[0];
    }
    return function piped(input) {
      return fns.reduce(function(prev, fn) {
        return fn(prev);
      }, input);
    };
  }
  var Observable = (function() {
    function Observable2(subscribe) {
      if (subscribe) {
        this._subscribe = subscribe;
      }
    }
    Observable2.prototype.lift = function(operator) {
      var observable2 = new Observable2();
      observable2.source = this;
      observable2.operator = operator;
      return observable2;
    };
    Observable2.prototype.subscribe = function(observerOrNext, error, complete) {
      var _this = this;
      var subscriber = isSubscriber(observerOrNext) ? observerOrNext : new SafeSubscriber(observerOrNext, error, complete);
      errorContext(function() {
        var _a = _this, operator = _a.operator, source = _a.source;
        subscriber.add(operator ? operator.call(subscriber, source) : source ? _this._subscribe(subscriber) : _this._trySubscribe(subscriber));
      });
      return subscriber;
    };
    Observable2.prototype._trySubscribe = function(sink) {
      try {
        return this._subscribe(sink);
      } catch (err) {
        sink.error(err);
      }
    };
    Observable2.prototype.forEach = function(next, promiseCtor) {
      var _this = this;
      promiseCtor = getPromiseCtor(promiseCtor);
      return new promiseCtor(function(resolve, reject) {
        var subscriber = new SafeSubscriber({
          next: function(value) {
            try {
              next(value);
            } catch (err) {
              reject(err);
              subscriber.unsubscribe();
            }
          },
          error: reject,
          complete: resolve
        });
        _this.subscribe(subscriber);
      });
    };
    Observable2.prototype._subscribe = function(subscriber) {
      var _a;
      return (_a = this.source) === null || _a === void 0 ? void 0 : _a.subscribe(subscriber);
    };
    Observable2.prototype[observable] = function() {
      return this;
    };
    Observable2.prototype.pipe = function() {
      var operations = [];
      for (var _i = 0; _i < arguments.length; _i++) {
        operations[_i] = arguments[_i];
      }
      return pipeFromArray(operations)(this);
    };
    Observable2.prototype.toPromise = function(promiseCtor) {
      var _this = this;
      promiseCtor = getPromiseCtor(promiseCtor);
      return new promiseCtor(function(resolve, reject) {
        var value;
        _this.subscribe(function(x2) {
          return value = x2;
        }, function(err) {
          return reject(err);
        }, function() {
          return resolve(value);
        });
      });
    };
    Observable2.create = function(subscribe) {
      return new Observable2(subscribe);
    };
    return Observable2;
  })();
  function getPromiseCtor(promiseCtor) {
    var _a;
    return (_a = promiseCtor !== null && promiseCtor !== void 0 ? promiseCtor : config.Promise) !== null && _a !== void 0 ? _a : Promise;
  }
  function isObserver(value) {
    return value && isFunction(value.next) && isFunction(value.error) && isFunction(value.complete);
  }
  function isSubscriber(value) {
    return value && value instanceof Subscriber || isObserver(value) && isSubscription(value);
  }
  var ObjectUnsubscribedError = createErrorClass(function(_super) {
    return function ObjectUnsubscribedErrorImpl() {
      _super(this);
      this.name = "ObjectUnsubscribedError";
      this.message = "object unsubscribed";
    };
  });
  var Subject = (function(_super) {
    __extends(Subject2, _super);
    function Subject2() {
      var _this = _super.call(this) || this;
      _this.closed = false;
      _this.currentObservers = null;
      _this.observers = [];
      _this.isStopped = false;
      _this.hasError = false;
      _this.thrownError = null;
      return _this;
    }
    Subject2.prototype.lift = function(operator) {
      var subject = new AnonymousSubject(this, this);
      subject.operator = operator;
      return subject;
    };
    Subject2.prototype._throwIfClosed = function() {
      if (this.closed) {
        throw new ObjectUnsubscribedError();
      }
    };
    Subject2.prototype.next = function(value) {
      var _this = this;
      errorContext(function() {
        var e_1, _a;
        _this._throwIfClosed();
        if (!_this.isStopped) {
          if (!_this.currentObservers) {
            _this.currentObservers = Array.from(_this.observers);
          }
          try {
            for (var _b = __values(_this.currentObservers), _c = _b.next(); !_c.done; _c = _b.next()) {
              var observer = _c.value;
              observer.next(value);
            }
          } catch (e_1_1) {
            e_1 = { error: e_1_1 };
          } finally {
            try {
              if (_c && !_c.done && (_a = _b.return)) _a.call(_b);
            } finally {
              if (e_1) throw e_1.error;
            }
          }
        }
      });
    };
    Subject2.prototype.error = function(err) {
      var _this = this;
      errorContext(function() {
        _this._throwIfClosed();
        if (!_this.isStopped) {
          _this.hasError = _this.isStopped = true;
          _this.thrownError = err;
          var observers = _this.observers;
          while (observers.length) {
            observers.shift().error(err);
          }
        }
      });
    };
    Subject2.prototype.complete = function() {
      var _this = this;
      errorContext(function() {
        _this._throwIfClosed();
        if (!_this.isStopped) {
          _this.isStopped = true;
          var observers = _this.observers;
          while (observers.length) {
            observers.shift().complete();
          }
        }
      });
    };
    Subject2.prototype.unsubscribe = function() {
      this.isStopped = this.closed = true;
      this.observers = this.currentObservers = null;
    };
    Object.defineProperty(Subject2.prototype, "observed", {
      get: function() {
        var _a;
        return ((_a = this.observers) === null || _a === void 0 ? void 0 : _a.length) > 0;
      },
      enumerable: false,
      configurable: true
    });
    Subject2.prototype._trySubscribe = function(subscriber) {
      this._throwIfClosed();
      return _super.prototype._trySubscribe.call(this, subscriber);
    };
    Subject2.prototype._subscribe = function(subscriber) {
      this._throwIfClosed();
      this._checkFinalizedStatuses(subscriber);
      return this._innerSubscribe(subscriber);
    };
    Subject2.prototype._innerSubscribe = function(subscriber) {
      var _this = this;
      var _a = this, hasError = _a.hasError, isStopped = _a.isStopped, observers = _a.observers;
      if (hasError || isStopped) {
        return EMPTY_SUBSCRIPTION;
      }
      this.currentObservers = null;
      observers.push(subscriber);
      return new Subscription(function() {
        _this.currentObservers = null;
        arrRemove(observers, subscriber);
      });
    };
    Subject2.prototype._checkFinalizedStatuses = function(subscriber) {
      var _a = this, hasError = _a.hasError, thrownError = _a.thrownError, isStopped = _a.isStopped;
      if (hasError) {
        subscriber.error(thrownError);
      } else if (isStopped) {
        subscriber.complete();
      }
    };
    Subject2.prototype.asObservable = function() {
      var observable2 = new Observable();
      observable2.source = this;
      return observable2;
    };
    Subject2.create = function(destination, source) {
      return new AnonymousSubject(destination, source);
    };
    return Subject2;
  })(Observable);
  var AnonymousSubject = (function(_super) {
    __extends(AnonymousSubject2, _super);
    function AnonymousSubject2(destination, source) {
      var _this = _super.call(this) || this;
      _this.destination = destination;
      _this.source = source;
      return _this;
    }
    AnonymousSubject2.prototype.next = function(value) {
      var _a, _b;
      (_b = (_a = this.destination) === null || _a === void 0 ? void 0 : _a.next) === null || _b === void 0 ? void 0 : _b.call(_a, value);
    };
    AnonymousSubject2.prototype.error = function(err) {
      var _a, _b;
      (_b = (_a = this.destination) === null || _a === void 0 ? void 0 : _a.error) === null || _b === void 0 ? void 0 : _b.call(_a, err);
    };
    AnonymousSubject2.prototype.complete = function() {
      var _a, _b;
      (_b = (_a = this.destination) === null || _a === void 0 ? void 0 : _a.complete) === null || _b === void 0 ? void 0 : _b.call(_a);
    };
    AnonymousSubject2.prototype._subscribe = function(subscriber) {
      var _a, _b;
      return (_b = (_a = this.source) === null || _a === void 0 ? void 0 : _a.subscribe(subscriber)) !== null && _b !== void 0 ? _b : EMPTY_SUBSCRIPTION;
    };
    return AnonymousSubject2;
  })(Subject);
  var BehaviorSubject = (function(_super) {
    __extends(BehaviorSubject2, _super);
    function BehaviorSubject2(_value) {
      var _this = _super.call(this) || this;
      _this._value = _value;
      return _this;
    }
    Object.defineProperty(BehaviorSubject2.prototype, "value", {
      get: function() {
        return this.getValue();
      },
      enumerable: false,
      configurable: true
    });
    BehaviorSubject2.prototype._subscribe = function(subscriber) {
      var subscription = _super.prototype._subscribe.call(this, subscriber);
      !subscription.closed && subscriber.next(this._value);
      return subscription;
    };
    BehaviorSubject2.prototype.getValue = function() {
      var _a = this, hasError = _a.hasError, thrownError = _a.thrownError, _value = _a._value;
      if (hasError) {
        throw thrownError;
      }
      this._throwIfClosed();
      return _value;
    };
    BehaviorSubject2.prototype.next = function(value) {
      _super.prototype.next.call(this, this._value = value);
    };
    return BehaviorSubject2;
  })(Subject);
  var _templateObject$2;
  var _templateObject2$2;
  function ownKeys$22(object, enumerableOnly) {
    var keys2 = Object.keys(object);
    if (Object.getOwnPropertySymbols) {
      var symbols = Object.getOwnPropertySymbols(object);
      enumerableOnly && (symbols = symbols.filter(function(sym) {
        return Object.getOwnPropertyDescriptor(object, sym).enumerable;
      })), keys2.push.apply(keys2, symbols);
    }
    return keys2;
  }
  function _objectSpread$22(target) {
    for (var i7 = 1; i7 < arguments.length; i7++) {
      var source = null != arguments[i7] ? arguments[i7] : {};
      i7 % 2 ? ownKeys$22(Object(source), true).forEach(function(key) {
        _defineProperty(target, key, source[key]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(target, Object.getOwnPropertyDescriptors(source)) : ownKeys$22(Object(source)).forEach(function(key) {
        Object.defineProperty(target, key, Object.getOwnPropertyDescriptor(source, key));
      });
    }
    return target;
  }
  function _createForOfIteratorHelper$13(o7, allowArrayLike) {
    var it = typeof Symbol !== "undefined" && o7[Symbol.iterator] || o7["@@iterator"];
    if (!it) {
      if (Array.isArray(o7) || (it = _unsupportedIterableToArray$13(o7)) || allowArrayLike && o7 && typeof o7.length === "number") {
        if (it) o7 = it;
        var i7 = 0;
        var F = function F2() {
        };
        return { s: F, n: function n6() {
          if (i7 >= o7.length) return { done: true };
          return { done: false, value: o7[i7++] };
        }, e: function e7(_e) {
          throw _e;
        }, f: F };
      }
      throw new TypeError("Invalid attempt to iterate non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
    }
    var normalCompletion = true, didErr = false, err;
    return { s: function s4() {
      it = it.call(o7);
    }, n: function n6() {
      var step = it.next();
      normalCompletion = step.done;
      return step;
    }, e: function e7(_e2) {
      didErr = true;
      err = _e2;
    }, f: function f3() {
      try {
        if (!normalCompletion && it["return"] != null) it["return"]();
      } finally {
        if (didErr) throw err;
      }
    } };
  }
  function _unsupportedIterableToArray$13(o7, minLen) {
    if (!o7) return;
    if (typeof o7 === "string") return _arrayLikeToArray$13(o7, minLen);
    var n6 = Object.prototype.toString.call(o7).slice(8, -1);
    if (n6 === "Object" && o7.constructor) n6 = o7.constructor.name;
    if (n6 === "Map" || n6 === "Set") return Array.from(o7);
    if (n6 === "Arguments" || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(n6)) return _arrayLikeToArray$13(o7, minLen);
  }
  function _arrayLikeToArray$13(arr, len) {
    if (len == null || len > arr.length) len = arr.length;
    for (var i7 = 0, arr2 = new Array(len); i7 < len; i7++) arr2[i7] = arr[i7];
    return arr2;
  }
  function _createSuper$3(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$3();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$3() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$2(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$2();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$2(r5.d.map(_createElementDescriptor$2)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$2() {
    _getDecoratorsApi$2 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$2(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$2(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$2(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$2(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$2(def) {
    var key = _toPropertyKey$2(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$2(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$2(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$2(element.descriptor) || _isDataDescriptor$2(other.descriptor)) {
          if (_hasDecorators$2(element) || _hasDecorators$2(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$2(element)) {
            if (_hasDecorators$2(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$2(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$2(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$2(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$2(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$2(arg) {
    var key = _toPrimitive$2(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$2(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var Transformer = _decorate$2(null, function(_initialize, _LitElement) {
    var Transformer2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(Transformer3, _LitElement2);
      var _super = _createSuper$3(Transformer3);
      function Transformer3(editor) {
        var _this;
        _classCallCheck(this, Transformer3);
        _this = _super.call(this);
        _initialize(_assertThisInitialized(_this));
        _this.editor = editor;
        _this.processIO(_this.constructor.inputs, _this.inputs);
        _this.processIO(_this.constructor.outputs, _this.outputs);
        _this.processIntermediates(_this.constructor.intermediates);
        _this.transform();
        _this.editor.addPipe(function(context2) {
          if (context2.type === "connectioncreated") {
            try {
              if (context2.data.target === _this.nodeId) {
                _this.subscribe(context2.data);
              }
            } catch (error) {
              alert(error.message);
              _this.editor.removeConnection(context2.data.id);
            }
          } else if (context2.type === "connectionremoved") {
            _this.unsubscribe(context2.data);
          }
        });
        return _this;
      }
      return _createClass(Transformer3);
    })(_LitElement);
    return {
      F: Transformer2,
      d: [{
        kind: "field",
        "static": true,
        key: "socket",
        value: function value() {
          return new classic.Socket("socket");
        }
      }, {
        kind: "field",
        "static": true,
        key: "inputs",
        value: function value() {
          return [];
        }
      }, {
        kind: "field",
        "static": true,
        key: "outputs",
        value: function value() {
          return [];
        }
      }, {
        kind: "field",
        "static": true,
        key: "intermediates",
        value: function value() {
          return [];
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject$2 || (_templateObject$2 = _taggedTemplateLiteral(["\n        :host {\n            min-width: 100px;\n            min-height: 100px;\n            display: block;\n        }\n    "])));
        }
      }, {
        kind: "field",
        decorators: [n4({
          attribute: "node-id",
          type: String
        })],
        key: "nodeId",
        value: void 0
      }, {
        kind: "field",
        decorators: [n4({
          type: Boolean
        })],
        key: "selected",
        value: void 0
      }, {
        kind: "field",
        key: "inputs",
        value: function value() {
          return {};
        }
      }, {
        kind: "field",
        key: "outputs",
        value: function value() {
          return {};
        }
      }, {
        kind: "field",
        key: "intermediates",
        value: function value() {
          return {};
        }
      }, {
        kind: "method",
        key: "processIO",
        value: function processIO(definitions, ioObject) {
          var _this2 = this;
          var _iterator = _createForOfIteratorHelper$13(definitions), _step;
          try {
            for (_iterator.s(); !(_step = _iterator.n()).done; ) {
              var def = _step.value;
              var subject = new BehaviorSubject(null);
              subject.subscribe(function() {
                return _this2.requestUpdate();
              });
              ioObject[def.label] = _objectSpread$22(_objectSpread$22({}, def), {}, {
                subject,
                socket: this.constructor.socket,
                validate: this.createValidateFunction(def)
              });
            }
          } catch (err) {
            _iterator.e(err);
          } finally {
            _iterator.f();
          }
        }
      }, {
        kind: "method",
        key: "createValidateFunction",
        value: function createValidateFunction(inputDef) {
          return function(outputDef) {
            if (!inputDef.schema) return true;
            if (!outputDef.schema) return false;
            return true;
          };
        }
      }, {
        kind: "method",
        key: "processIntermediates",
        value: function processIntermediates(list) {
          var _iterator2 = _createForOfIteratorHelper$13(list), _step2;
          try {
            for (_iterator2.s(); !(_step2 = _iterator2.n()).done; ) {
              var item = _step2.value;
              this.intermediates[item.label] = item;
            }
          } catch (err) {
            _iterator2.e(err);
          } finally {
            _iterator2.f();
          }
        }
      }, {
        kind: "method",
        key: "transform",
        value: function transform() {
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          return b2(_templateObject2$2 || (_templateObject2$2 = _taggedTemplateLiteral([""])));
        }
      }, {
        kind: "method",
        key: "subscribe",
        value: function subscribe(context2) {
          if (context2.target !== this.nodeId) return;
          var sourceNode = this.editor.getNode(context2.source);
          var sourceOutput = sourceNode.outputs[context2.sourceOutput];
          var targetInput = this.inputs[context2.targetInput];
          if (targetInput.subscription) {
            throw new Error("Input already has a subscription.");
          }
          if (!targetInput.validate(sourceOutput)) {
            throw new Error("Schema validation failed.");
          }
          targetInput.subscription = sourceOutput.subject.subscribe(function(value) {
            targetInput.subject.next(value);
          });
        }
      }, {
        kind: "method",
        key: "unsubscribe",
        value: function unsubscribe(context2) {
          if (context2.target !== this.nodeId) return;
          var targetInput = this.inputs[context2.targetInput];
          if (targetInput.subscription) {
            targetInput.subscription.unsubscribe();
            delete targetInput.subscription;
          }
        }
      }]
    };
  }, i4);
  var _templateObject$1;
  var _templateObject2$1;
  var _templateObject3;
  var _templateObject4;
  function _createSuper$2(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$2();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$2() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate$1(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi$1();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements$1(r5.d.map(_createElementDescriptor$1)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi$1() {
    _getDecoratorsApi$1 = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators$1(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey$1(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty$1(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty$1(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor$1(def) {
    var key = _toPropertyKey$1(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter$1(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements$1(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor$1(element.descriptor) || _isDataDescriptor$1(other.descriptor)) {
          if (_hasDecorators$1(element) || _hasDecorators$1(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators$1(element)) {
            if (_hasDecorators$1(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter$1(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators$1(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor$1(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty$1(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey$1(arg) {
    var key = _toPrimitive$1(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive$1(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var TransformerNode = _decorate$1([t3("transformer-node")], function(_initialize, _LitElement) {
    var TransformerNode2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(TransformerNode3, _LitElement2);
      var _super = _createSuper$2(TransformerNode3);
      function TransformerNode3() {
        var _this;
        _classCallCheck(this, TransformerNode3);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(TransformerNode3);
    })(_LitElement);
    return {
      F: TransformerNode2,
      d: [{
        kind: "get",
        decorators: [n4({
          type: Object
        })],
        key: "data",
        value: function data() {
          return this._data;
        }
      }, {
        kind: "set",
        key: "data",
        value: function data(val) {
          var oldVal = this._data;
          this._data = val;
          if (val && !oldVal) {
            this.appendChild(val);
          }
          this.requestUpdate("data", oldVal);
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "emit",
        value: function value() {
          return function() {
            throw new Error("emit not set properly");
          };
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: String
        })],
        key: "seed",
        value: function value() {
          return "";
        }
      }, {
        kind: "field",
        key: "_data",
        value: void 0
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return [vars$1, i(_templateObject$1 || (_templateObject$1 = _taggedTemplateLiteral(["\n            :host {\n                display: block;\n            }\n\n            .node {\n                display: flex;\n                flex-direction: column;\n                background: var(--node-color);\n                border: 2px solid #4e58bf;\n                border-radius: 10px;\n                cursor: pointer;\n                box-sizing: border-box;\n                width: var(--node-width);\n                height: auto;\n                padding-bottom: 6px;\n                position: relative;\n                user-select: none;\n            }\n\n            .inputs,\n            .outputs {\n                display: flex;\n                flex-direction: row;\n            }\n\n            .inputs .input-socket {\n                margin-top: calc(\n                    (var(--socket-size) / -2) - var(--socket-margin)\n                );\n            }\n\n            .outputs .output-socket {\n                margin-bottom: calc(\n                    (var(--socket-size) / -2) - var(--socket-margin)\n                );\n            }\n\n            .input {\n                display: flex;\n                flex-direction: column;\n            }\n\n            .output {\n                display: flex;\n                flex-direction: column-reverse;\n            }\n\n            /* You can add other CSS rules here as needed */\n        "])))];
        }
      }, {
        kind: "method",
        key: "render",
        value: (
          // eslint-disable-next-line @typescript-eslint/naming-convention
          function render() {
            var _this$data, _this2 = this, _this$data2;
            return b2(_templateObject2$1 || (_templateObject2$1 = _taggedTemplateLiteral(['\n            <div class="node">\n                <div class="inputs">\n                    ', '\n                </div>\n\n                <slot></slot>\n\n                <div class="outputs">\n                    ', "\n                </div>\n            </div>\n        "])), Object.entries(((_this$data = this.data) === null || _this$data === void 0 ? void 0 : _this$data.inputs) || {}).map(function(_ref) {
              var _this2$data;
              var _ref2 = _slicedToArray(_ref, 2), key = _ref2[0], input = _ref2[1];
              return b2(_templateObject3 || (_templateObject3 = _taggedTemplateLiteral(['\n                            <div class="input">\n                                <ref-element\n                                    class="input-socket"\n                                    .emit=', "\n                                    .data=", '\n                                ></ref-element>\n                                <div class="input-title">', "</div>\n                            </div>\n                        "])), _this2.emit, {
                type: "socket",
                side: "input",
                key,
                nodeId: (_this2$data = _this2.data) === null || _this2$data === void 0 ? void 0 : _this2$data.nodeId,
                payload: input.socket
              }, input.label);
            }), Object.entries(((_this$data2 = this.data) === null || _this$data2 === void 0 ? void 0 : _this$data2.outputs) || {}).map(function(_ref3) {
              var _this2$data2;
              var _ref4 = _slicedToArray(_ref3, 2), key = _ref4[0], output = _ref4[1];
              return b2(_templateObject4 || (_templateObject4 = _taggedTemplateLiteral(['\n                            <div class="output">\n                                <div class="output-title">', '</div>\n                                <ref-element\n                                    class="output-socket"\n                                    .emit=', "\n                                    .data=", "\n                                ></ref-element>\n                            </div>\n                        "])), output.label, _this2.emit, {
                type: "socket",
                side: "output",
                key,
                nodeId: (_this2$data2 = _this2.data) === null || _this2$data2 === void 0 ? void 0 : _this2$data2.nodeId,
                payload: output.socket
              });
            }));
          }
        )
      }]
    };
  }, i4);
  var _templateObject;
  var _templateObject2;
  function ownKeys$12(object, enumerableOnly) {
    var keys2 = Object.keys(object);
    if (Object.getOwnPropertySymbols) {
      var symbols = Object.getOwnPropertySymbols(object);
      enumerableOnly && (symbols = symbols.filter(function(sym) {
        return Object.getOwnPropertyDescriptor(object, sym).enumerable;
      })), keys2.push.apply(keys2, symbols);
    }
    return keys2;
  }
  function _objectSpread$12(target) {
    for (var i7 = 1; i7 < arguments.length; i7++) {
      var source = null != arguments[i7] ? arguments[i7] : {};
      i7 % 2 ? ownKeys$12(Object(source), true).forEach(function(key) {
        _defineProperty(target, key, source[key]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(target, Object.getOwnPropertyDescriptors(source)) : ownKeys$12(Object(source)).forEach(function(key) {
        Object.defineProperty(target, key, Object.getOwnPropertyDescriptor(source, key));
      });
    }
    return target;
  }
  function _createSuper$1(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct$14();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct$14() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  function _decorate(decorators, factory, superClass, mixins) {
    var api = _getDecoratorsApi();
    if (mixins) {
      for (var i7 = 0; i7 < mixins.length; i7++) {
        api = mixins[i7](api);
      }
    }
    var r5 = factory(function initialize(O) {
      api.initializeInstanceElements(O, decorated.elements);
    }, superClass);
    var decorated = api.decorateClass(_coalesceClassElements(r5.d.map(_createElementDescriptor)), decorators);
    api.initializeClassElements(r5.F, decorated.elements);
    return api.runClassFinishers(r5.F, decorated.finishers);
  }
  function _getDecoratorsApi() {
    _getDecoratorsApi = function _getDecoratorsApi2() {
      return api;
    };
    var api = { elementsDefinitionOrder: [["method"], ["field"]], initializeInstanceElements: function initializeInstanceElements(O, elements) {
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          if (element.kind === kind && element.placement === "own") {
            this.defineClassElement(O, element);
          }
        }, this);
      }, this);
    }, initializeClassElements: function initializeClassElements(F, elements) {
      var proto = F.prototype;
      ["method", "field"].forEach(function(kind) {
        elements.forEach(function(element) {
          var placement = element.placement;
          if (element.kind === kind && (placement === "static" || placement === "prototype")) {
            var receiver = placement === "static" ? F : proto;
            this.defineClassElement(receiver, element);
          }
        }, this);
      }, this);
    }, defineClassElement: function defineClassElement(receiver, element) {
      var descriptor = element.descriptor;
      if (element.kind === "field") {
        var initializer = element.initializer;
        descriptor = { enumerable: descriptor.enumerable, writable: descriptor.writable, configurable: descriptor.configurable, value: initializer === void 0 ? void 0 : initializer.call(receiver) };
      }
      Object.defineProperty(receiver, element.key, descriptor);
    }, decorateClass: function decorateClass(elements, decorators) {
      var newElements = [];
      var finishers = [];
      var placements = { "static": [], prototype: [], own: [] };
      elements.forEach(function(element) {
        this.addElementPlacement(element, placements);
      }, this);
      elements.forEach(function(element) {
        if (!_hasDecorators(element)) return newElements.push(element);
        var elementFinishersExtras = this.decorateElement(element, placements);
        newElements.push(elementFinishersExtras.element);
        newElements.push.apply(newElements, elementFinishersExtras.extras);
        finishers.push.apply(finishers, elementFinishersExtras.finishers);
      }, this);
      if (!decorators) {
        return { elements: newElements, finishers };
      }
      var result = this.decorateConstructor(newElements, decorators);
      finishers.push.apply(finishers, result.finishers);
      result.finishers = finishers;
      return result;
    }, addElementPlacement: function addElementPlacement(element, placements, silent) {
      var keys2 = placements[element.placement];
      if (!silent && keys2.indexOf(element.key) !== -1) {
        throw new TypeError("Duplicated element (" + element.key + ")");
      }
      keys2.push(element.key);
    }, decorateElement: function decorateElement(element, placements) {
      var extras = [];
      var finishers = [];
      for (var decorators = element.decorators, i7 = decorators.length - 1; i7 >= 0; i7--) {
        var keys2 = placements[element.placement];
        keys2.splice(keys2.indexOf(element.key), 1);
        var elementObject = this.fromElementDescriptor(element);
        var elementFinisherExtras = this.toElementFinisherExtras((0, decorators[i7])(elementObject) || elementObject);
        element = elementFinisherExtras.element;
        this.addElementPlacement(element, placements);
        if (elementFinisherExtras.finisher) {
          finishers.push(elementFinisherExtras.finisher);
        }
        var newExtras = elementFinisherExtras.extras;
        if (newExtras) {
          for (var j = 0; j < newExtras.length; j++) {
            this.addElementPlacement(newExtras[j], placements);
          }
          extras.push.apply(extras, newExtras);
        }
      }
      return { element, finishers, extras };
    }, decorateConstructor: function decorateConstructor(elements, decorators) {
      var finishers = [];
      for (var i7 = decorators.length - 1; i7 >= 0; i7--) {
        var obj = this.fromClassDescriptor(elements);
        var elementsAndFinisher = this.toClassDescriptor((0, decorators[i7])(obj) || obj);
        if (elementsAndFinisher.finisher !== void 0) {
          finishers.push(elementsAndFinisher.finisher);
        }
        if (elementsAndFinisher.elements !== void 0) {
          elements = elementsAndFinisher.elements;
          for (var j = 0; j < elements.length - 1; j++) {
            for (var k2 = j + 1; k2 < elements.length; k2++) {
              if (elements[j].key === elements[k2].key && elements[j].placement === elements[k2].placement) {
                throw new TypeError("Duplicated element (" + elements[j].key + ")");
              }
            }
          }
        }
      }
      return { elements, finishers };
    }, fromElementDescriptor: function fromElementDescriptor(element) {
      var obj = { kind: element.kind, key: element.key, placement: element.placement, descriptor: element.descriptor };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      if (element.kind === "field") obj.initializer = element.initializer;
      return obj;
    }, toElementDescriptors: function toElementDescriptors(elementObjects) {
      if (elementObjects === void 0) return;
      return _toArray(elementObjects).map(function(elementObject) {
        var element = this.toElementDescriptor(elementObject);
        this.disallowProperty(elementObject, "finisher", "An element descriptor");
        this.disallowProperty(elementObject, "extras", "An element descriptor");
        return element;
      }, this);
    }, toElementDescriptor: function toElementDescriptor(elementObject) {
      var kind = String(elementObject.kind);
      if (kind !== "method" && kind !== "field") {
        throw new TypeError(`An element descriptor's .kind property must be either "method" or "field", but a decorator created an element descriptor with .kind "` + kind + '"');
      }
      var key = _toPropertyKey(elementObject.key);
      var placement = String(elementObject.placement);
      if (placement !== "static" && placement !== "prototype" && placement !== "own") {
        throw new TypeError(`An element descriptor's .placement property must be one of "static", "prototype" or "own", but a decorator created an element descriptor with .placement "` + placement + '"');
      }
      var descriptor = elementObject.descriptor;
      this.disallowProperty(elementObject, "elements", "An element descriptor");
      var element = { kind, key, placement, descriptor: Object.assign({}, descriptor) };
      if (kind !== "field") {
        this.disallowProperty(elementObject, "initializer", "A method descriptor");
      } else {
        this.disallowProperty(descriptor, "get", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "set", "The property descriptor of a field descriptor");
        this.disallowProperty(descriptor, "value", "The property descriptor of a field descriptor");
        element.initializer = elementObject.initializer;
      }
      return element;
    }, toElementFinisherExtras: function toElementFinisherExtras(elementObject) {
      var element = this.toElementDescriptor(elementObject);
      var finisher = _optionalCallableProperty(elementObject, "finisher");
      var extras = this.toElementDescriptors(elementObject.extras);
      return { element, finisher, extras };
    }, fromClassDescriptor: function fromClassDescriptor(elements) {
      var obj = { kind: "class", elements: elements.map(this.fromElementDescriptor, this) };
      var desc = { value: "Descriptor", configurable: true };
      Object.defineProperty(obj, Symbol.toStringTag, desc);
      return obj;
    }, toClassDescriptor: function toClassDescriptor(obj) {
      var kind = String(obj.kind);
      if (kind !== "class") {
        throw new TypeError(`A class descriptor's .kind property must be "class", but a decorator created a class descriptor with .kind "` + kind + '"');
      }
      this.disallowProperty(obj, "key", "A class descriptor");
      this.disallowProperty(obj, "placement", "A class descriptor");
      this.disallowProperty(obj, "descriptor", "A class descriptor");
      this.disallowProperty(obj, "initializer", "A class descriptor");
      this.disallowProperty(obj, "extras", "A class descriptor");
      var finisher = _optionalCallableProperty(obj, "finisher");
      var elements = this.toElementDescriptors(obj.elements);
      return { elements, finisher };
    }, runClassFinishers: function runClassFinishers(constructor, finishers) {
      for (var i7 = 0; i7 < finishers.length; i7++) {
        var newConstructor = (0, finishers[i7])(constructor);
        if (newConstructor !== void 0) {
          if (typeof newConstructor !== "function") {
            throw new TypeError("Finishers must return a constructor.");
          }
          constructor = newConstructor;
        }
      }
      return constructor;
    }, disallowProperty: function disallowProperty(obj, name, objectType) {
      if (obj[name] !== void 0) {
        throw new TypeError(objectType + " can't have a ." + name + " property.");
      }
    } };
    return api;
  }
  function _createElementDescriptor(def) {
    var key = _toPropertyKey(def.key);
    var descriptor;
    if (def.kind === "method") {
      descriptor = { value: def.value, writable: true, configurable: true, enumerable: false };
    } else if (def.kind === "get") {
      descriptor = { get: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "set") {
      descriptor = { set: def.value, configurable: true, enumerable: false };
    } else if (def.kind === "field") {
      descriptor = { configurable: true, writable: true, enumerable: true };
    }
    var element = { kind: def.kind === "field" ? "field" : "method", key, placement: def["static"] ? "static" : def.kind === "field" ? "own" : "prototype", descriptor };
    if (def.decorators) element.decorators = def.decorators;
    if (def.kind === "field") element.initializer = def.value;
    return element;
  }
  function _coalesceGetterSetter(element, other) {
    if (element.descriptor.get !== void 0) {
      other.descriptor.get = element.descriptor.get;
    } else {
      other.descriptor.set = element.descriptor.set;
    }
  }
  function _coalesceClassElements(elements) {
    var newElements = [];
    var isSameElement = function isSameElement2(other2) {
      return other2.kind === "method" && other2.key === element.key && other2.placement === element.placement;
    };
    for (var i7 = 0; i7 < elements.length; i7++) {
      var element = elements[i7];
      var other;
      if (element.kind === "method" && (other = newElements.find(isSameElement))) {
        if (_isDataDescriptor(element.descriptor) || _isDataDescriptor(other.descriptor)) {
          if (_hasDecorators(element) || _hasDecorators(other)) {
            throw new ReferenceError("Duplicated methods (" + element.key + ") can't be decorated.");
          }
          other.descriptor = element.descriptor;
        } else {
          if (_hasDecorators(element)) {
            if (_hasDecorators(other)) {
              throw new ReferenceError("Decorators can't be placed on different accessors with for the same property (" + element.key + ").");
            }
            other.decorators = element.decorators;
          }
          _coalesceGetterSetter(element, other);
        }
      } else {
        newElements.push(element);
      }
    }
    return newElements;
  }
  function _hasDecorators(element) {
    return element.decorators && element.decorators.length;
  }
  function _isDataDescriptor(desc) {
    return desc !== void 0 && !(desc.value === void 0 && desc.writable === void 0);
  }
  function _optionalCallableProperty(obj, name) {
    var value = obj[name];
    if (value !== void 0 && typeof value !== "function") {
      throw new TypeError("Expected '" + name + "' to be a function");
    }
    return value;
  }
  function _toPropertyKey(arg) {
    var key = _toPrimitive(arg, "string");
    return _typeof(key) === "symbol" ? key : String(key);
  }
  function _toPrimitive(input, hint) {
    if (_typeof(input) !== "object" || input === null) return input;
    var prim = input[Symbol.toPrimitive];
    if (prim !== void 0) {
      var res = prim.call(input, hint || "default");
      if (_typeof(res) !== "object") return res;
      throw new TypeError("@@toPrimitive must return a primitive value.");
    }
    return (hint === "string" ? String : Number)(input);
  }
  var RefElement = _decorate([t3("ref-element")], function(_initialize, _LitElement) {
    var RefElement2 = /* @__PURE__ */ (function(_LitElement2) {
      _inherits(RefElement3, _LitElement2);
      var _super = _createSuper$1(RefElement3);
      function RefElement3() {
        var _this;
        _classCallCheck(this, RefElement3);
        for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
          args[_key] = arguments[_key];
        }
        _this = _super.call.apply(_super, [this].concat(args));
        _initialize(_assertThisInitialized(_this));
        return _this;
      }
      return _createClass(RefElement3);
    })(_LitElement);
    return {
      F: RefElement2,
      d: [{
        kind: "field",
        decorators: [n4({
          type: Object
        })],
        key: "data",
        value: function value() {
          return {};
        }
      }, {
        kind: "field",
        decorators: [n4({
          type: Function
        })],
        key: "emit",
        value: function value() {
          return function(_payload) {
            throw new Error("emit not set properly");
          };
        }
      }, {
        kind: "field",
        decorators: [e5("#element")],
        key: "element",
        value: void 0
      }, {
        kind: "field",
        key: "originalElementsFromPoint",
        value: function value() {
          return document.elementsFromPoint.bind(document);
        }
      }, {
        kind: "method",
        key: "updated",
        value: function updated() {
          this.emit({
            type: "render",
            data: _objectSpread$12(_objectSpread$12({}, this.data), {}, {
              element: this.element
            })
          });
        }
      }, {
        kind: "method",
        key: "disconnectedCallback",
        value: function disconnectedCallback() {
          this.emit({
            type: "unmount",
            data: {
              element: this.element
            }
          });
          _get(_getPrototypeOf(RefElement2.prototype), "disconnectedCallback", this).call(this);
        }
      }, {
        kind: "method",
        key: "handlePointerDown",
        value: function handlePointerDown() {
          var originalElementsFromPoint = this.originalElementsFromPoint;
          document.elementsFromPoint = function(x2, y3) {
            var elements = originalElementsFromPoint(x2, y3);
            var shadowElements = [];
            var shadowRoot = elements[0].shadowRoot;
            var d3 = 0;
            while (shadowRoot) {
              var _innerElements$;
              var innerElements = shadowRoot.elementsFromPoint(x2, y3);
              d3++;
              shadowElements = innerElements.concat(shadowElements);
              shadowRoot = (_innerElements$ = innerElements[0]) === null || _innerElements$ === void 0 ? void 0 : _innerElements$.shadowRoot;
              if (d3 > 5) break;
            }
            return Array.from(new Set(shadowElements.concat(elements)));
          };
          console.log("ref pointerdown");
        }
      }, {
        kind: "method",
        key: "handlePointerUp",
        value: function handlePointerUp() {
          var _this2 = this;
          setTimeout(function() {
            document.elementsFromPoint = _this2.originalElementsFromPoint;
          }, 0);
          console.log("ref pointerup");
        }
      }, {
        kind: "method",
        key: "render",
        value: function render() {
          return b2(_templateObject || (_templateObject = _taggedTemplateLiteral(['<div\n            id="element"\n            @pointerdown="', '"\n            @pointerup="', '"\n            .data=', "\n        ></div>"])), this.handlePointerDown, this.handlePointerUp, this.data);
        }
      }, {
        kind: "field",
        "static": true,
        key: "styles",
        value: function value() {
          return i(_templateObject2 || (_templateObject2 = _taggedTemplateLiteral(["\n        /* Add your styles here */\n    "])));
        }
      }]
    };
  }, i4);
  function _createForOfIteratorHelper3(o7, allowArrayLike) {
    var it = typeof Symbol !== "undefined" && o7[Symbol.iterator] || o7["@@iterator"];
    if (!it) {
      if (Array.isArray(o7) || (it = _unsupportedIterableToArray4(o7)) || allowArrayLike && o7 && typeof o7.length === "number") {
        if (it) o7 = it;
        var i7 = 0;
        var F = function F2() {
        };
        return { s: F, n: function n6() {
          if (i7 >= o7.length) return { done: true };
          return { done: false, value: o7[i7++] };
        }, e: function e7(_e) {
          throw _e;
        }, f: F };
      }
      throw new TypeError("Invalid attempt to iterate non-iterable instance.\nIn order to be iterable, non-array objects must have a [Symbol.iterator]() method.");
    }
    var normalCompletion = true, didErr = false, err;
    return { s: function s4() {
      it = it.call(o7);
    }, n: function n6() {
      var step = it.next();
      normalCompletion = step.done;
      return step;
    }, e: function e7(_e2) {
      didErr = true;
      err = _e2;
    }, f: function f3() {
      try {
        if (!normalCompletion && it["return"] != null) it["return"]();
      } finally {
        if (didErr) throw err;
      }
    } };
  }
  function _unsupportedIterableToArray4(o7, minLen) {
    if (!o7) return;
    if (typeof o7 === "string") return _arrayLikeToArray4(o7, minLen);
    var n6 = Object.prototype.toString.call(o7).slice(8, -1);
    if (n6 === "Object" && o7.constructor) n6 = o7.constructor.name;
    if (n6 === "Map" || n6 === "Set") return Array.from(o7);
    if (n6 === "Arguments" || /^(?:Ui|I)nt(?:8|16|32)(?:Clamped)?Array$/.test(n6)) return _arrayLikeToArray4(o7, minLen);
  }
  function _arrayLikeToArray4(arr, len) {
    if (len == null || len > arr.length) len = arr.length;
    for (var i7 = 0, arr2 = new Array(len); i7 < len; i7++) arr2[i7] = arr[i7];
    return arr2;
  }
  function ownKeys3(object, enumerableOnly) {
    var keys2 = Object.keys(object);
    if (Object.getOwnPropertySymbols) {
      var symbols = Object.getOwnPropertySymbols(object);
      enumerableOnly && (symbols = symbols.filter(function(sym) {
        return Object.getOwnPropertyDescriptor(object, sym).enumerable;
      })), keys2.push.apply(keys2, symbols);
    }
    return keys2;
  }
  function _objectSpread3(target) {
    for (var i7 = 1; i7 < arguments.length; i7++) {
      var source = null != arguments[i7] ? arguments[i7] : {};
      i7 % 2 ? ownKeys3(Object(source), true).forEach(function(key) {
        _defineProperty(target, key, source[key]);
      }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(target, Object.getOwnPropertyDescriptors(source)) : ownKeys3(Object(source)).forEach(function(key) {
        Object.defineProperty(target, key, Object.getOwnPropertyDescriptor(source, key));
      });
    }
    return target;
  }
  function _createSuper(Derived) {
    var hasNativeReflectConstruct = _isNativeReflectConstruct5();
    return function _createSuperInternal() {
      var Super = _getPrototypeOf(Derived), result;
      if (hasNativeReflectConstruct) {
        var NewTarget = _getPrototypeOf(this).constructor;
        result = Reflect.construct(Super, arguments, NewTarget);
      } else {
        result = Super.apply(this, arguments);
      }
      return _possibleConstructorReturn(this, result);
    };
  }
  function _isNativeReflectConstruct5() {
    if (typeof Reflect === "undefined" || !Reflect.construct) return false;
    if (Reflect.construct.sham) return false;
    if (typeof Proxy === "function") return true;
    try {
      Boolean.prototype.valueOf.call(Reflect.construct(Boolean, [], function() {
      }));
      return true;
    } catch (e7) {
      return false;
    }
  }
  var LitPlugin = /* @__PURE__ */ (function(_Scope) {
    _inherits(LitPlugin2, _Scope);
    var _super = _createSuper(LitPlugin2);
    function LitPlugin2() {
      var _this;
      _classCallCheck(this, LitPlugin2);
      _this = _super.call(this, "lit-render");
      _defineProperty(_assertThisInitialized(_this), "renderer", void 0);
      _defineProperty(_assertThisInitialized(_this), "presets", []);
      _defineProperty(_assertThisInitialized(_this), "owners", /* @__PURE__ */ new WeakMap());
      _this.renderer = getRenderer();
      _this.addPipe(function(context2) {
        if (!context2 || _typeof(context2) !== "object" || !("type" in context2)) return context2;
        if (context2.type === "unmount") {
          _this.unmount(context2.data.element);
        } else if (context2.type === "render") {
          if ("filled" in context2.data && context2.data.filled) {
            return context2;
          }
          if (_this.mount(context2.data.element, context2)) {
            return _objectSpread3(_objectSpread3({}, context2), {}, {
              data: _objectSpread3(_objectSpread3({}, context2.data), {}, {
                filled: true
              })
            });
          }
        }
        return context2;
      });
      return _this;
    }
    _createClass(LitPlugin2, [{
      key: "setParent",
      value: function setParent(scope) {
        var _this2 = this;
        _get(_getPrototypeOf(LitPlugin2.prototype), "setParent", this).call(this, scope);
        this.presets.forEach(function(preset) {
          if (preset.attach) preset.attach(_this2);
        });
      }
    }, {
      key: "unmount",
      value: function unmount(element) {
        this.owners["delete"](element);
        this.renderer.unmount(element);
      }
    }, {
      key: "mount",
      value: function mount(element, context2) {
        var _this3 = this;
        var existing = this.renderer.get(element);
        var parent = this.parentScope();
        if (existing) {
          this.presets.forEach(function(preset2) {
            if (_this3.owners.get(element) !== preset2) return;
            var result2 = preset2.update(context2, _this3);
            if (result2) {
              _this3.renderer.update(existing, result2);
            }
          });
          return true;
        }
        var _iterator = _createForOfIteratorHelper3(this.presets), _step;
        try {
          for (_iterator.s(); !(_step = _iterator.n()).done; ) {
            var preset = _step.value;
            var result = preset.render(context2, this);
            if (!result) continue;
            this.renderer.mount(element, result.component, result.props, function() {
              return parent === null || parent === void 0 ? void 0 : parent.emit({
                type: "rendered",
                data: context2.data
              });
            });
            this.owners.set(element, preset);
            return true;
          }
        } catch (err) {
          _iterator.e(err);
        } finally {
          _iterator.f();
        }
      }
    }, {
      key: "addPreset",
      value: function addPreset(preset) {
        var local = preset;
        if (local.attach) local.attach(this);
        this.presets.push(local);
      }
    }]);
    return LitPlugin2;
  })(Scope);

  // frontend/loops-editor-entry.js
  var KIND_COLORS = {
    trigger: "#34d399",
    agent: "#a78bfa",
    action: "#60a5fa",
    decision: "#fbbf24",
    human_approval: "#f87171",
    transform: "#22d3ee",
    retry: "#fb923c",
    parallel: "#e879f9",
    subflow: "#818cf8",
    output: "#2dd4bf"
  };
  var XnautWorkflowNode = class extends i4 {
    static properties = {
      data: { attribute: false },
      emit: { attribute: false },
      seed: {}
    };
    constructor() {
      super();
      this.data = null;
      this.emit = () => {
      };
      this.seed = "";
    }
    static styles = i`
    :host { display:block; width:220px; font-family:Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; letter-spacing:0; }
    .node { overflow:hidden; border:1px solid #444953; border-radius:7px; background:#20232a; color:#e7e9ee; box-shadow:0 8px 22px rgba(0,0,0,.26); cursor:pointer; user-select:none; }
    .node:hover { border-color:#626975; }
    .node.selected { border-color:var(--kind-color); box-shadow:0 0 0 1px var(--kind-color),0 10px 28px rgba(0,0,0,.32); }
    .head { display:flex; align-items:center; gap:9px; min-height:42px; padding:8px 10px; border-bottom:1px solid #343840; }
    .kind { width:8px; height:26px; flex:0 0 auto; border-radius:3px; background:var(--kind-color); }
    .copy { min-width:0; flex:1 1 auto; }
    .title { overflow:hidden; font-size:12px; font-weight:700; text-overflow:ellipsis; white-space:nowrap; }
    .type { margin-top:2px; color:#8d939f; font-size:9px; font-weight:650; text-transform:uppercase; }
    .status { width:8px; height:8px; flex:0 0 auto; border-radius:50%; background:#69707c; }
    .status[data-status="ready"] { background:#a78bfa; }
    .status[data-status="running"] { border:2px solid rgba(52,211,153,.3); border-top-color:#34d399; background:transparent; animation:spin .8s linear infinite; }
    .status[data-status="waiting_for_approval"] { background:#fbbf24; }
    .status[data-status="completed"] { background:#60a5fa; }
    .status[data-status="failed"] { background:#f87171; }
    .ports { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr); gap:8px; padding:8px 0 9px; }
    .inputs,.outputs { display:flex; flex-direction:column; gap:6px; }
    .port { display:flex; align-items:center; min-height:20px; color:#aab0ba; font-size:10px; }
    .output { justify-content:flex-end; text-align:right; }
    .label { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
    ref-element { display:inline-flex; align-items:center; }
    .input ref-element { margin-left:-9px; margin-right:5px; }
    .output ref-element { margin-left:5px; margin-right:-9px; }
    .empty { min-height:18px; }
    @keyframes spin { to { transform:rotate(360deg); } }
  `;
    portEntries(side) {
      return Object.entries(this.data?.[side] || {}).sort((a3, b3) => (a3[1]?.index || 0) - (b3[1]?.index || 0));
    }
    socketRef(side, key, port) {
      return b2`<ref-element
      .data=${{ type: "socket", side, key, nodeId: this.data?.id, payload: port.socket }}
      .emit=${this.emit}
    ></ref-element>`;
    }
    render() {
      const meta = this.data?.xnaut || {};
      const kind = meta.kind || "action";
      const color = KIND_COLORS[kind] || "#60a5fa";
      const inputs = this.portEntries("inputs");
      const outputs = this.portEntries("outputs");
      return b2`<div class="node ${this.data?.selected ? "selected" : ""}" style="--kind-color:${color}">
      <div class="head">
        <span class="kind"></span>
        <span class="copy"><span class="title">${this.data?.label || meta.name || "Node"}</span><span class="type">${kind.replaceAll("_", " ")}</span></span>
        <span class="status" data-status=${meta.runStatus || "pending"}></span>
      </div>
      <div class="ports">
        <div class="inputs">${inputs.length ? inputs.map(([key, port]) => b2`<div class="port input">${this.socketRef("input", key, port)}<span class="label">${port.label || key}</span></div>`) : b2`<span class="empty"></span>`}</div>
        <div class="outputs">${outputs.length ? outputs.map(([key, port]) => b2`<div class="port output"><span class="label">${port.label || key}</span>${this.socketRef("output", key, port)}</div>`) : b2`<span class="empty"></span>`}</div>
      </div>
    </div>`;
    }
  };
  function clone(value) {
    return JSON.parse(JSON.stringify(value));
  }
  function normalizeNode(definition) {
    return {
      id: definition.id,
      kind: definition.kind || "action",
      name: definition.name || definition.kind || "Node",
      inputs: clone(definition.inputs || []),
      outputs: clone(definition.outputs || []),
      config: clone(definition.config ?? null),
      permissions: clone(definition.permissions || []),
      permission_layers: clone(definition.permission_layers || []),
      model_policy: clone(definition.model_policy ?? null),
      timeout_seconds: definition.timeout_seconds ?? null,
      max_retries: Number(definition.max_retries || 0)
    };
  }
  async function createEditor(container, hooks = {}) {
    const editor = new NodeEditor();
    const area = new AreaPlugin(container);
    const connection = new ConnectionPlugin();
    const render = new LitPlugin();
    const sockets = /* @__PURE__ */ new Map();
    let loading = false;
    let selectedId = "";
    render.addPreset(index3.classic.setup({
      customize: { node: () => XnautWorkflowNode }
    }));
    connection.addPreset(index2.classic.setup());
    editor.use(area);
    area.use(connection);
    area.use(render);
    index.simpleNodesOrder(area);
    const selector2 = index.selector();
    const accumulating = index.accumulateOnCtrl();
    index.selectableNodes(area, selector2, { accumulating });
    const notifyChange = () => {
      if (!loading) hooks.onChange?.();
    };
    editor.addPipe((context2) => {
      if (["nodecreated", "noderemoved", "connectioncreated", "connectionremoved"].includes(context2.type)) notifyChange();
      return context2;
    });
    area.addPipe((context2) => {
      if (context2.type === "nodepicked") {
        selectedId = context2.data.id;
        hooks.onSelect?.(selectedId, getNode(selectedId));
      }
      if (context2.type === "nodetranslated" || context2.type === "zoomed" || context2.type === "translated") notifyChange();
      return context2;
    });
    function socket(dataType) {
      const key = dataType || "any";
      if (!sockets.has(key)) sockets.set(key, new classic.Socket(key));
      return sockets.get(key);
    }
    function toReteNode(definition) {
      const normalized = normalizeNode(definition);
      const node = new classic.Node(normalized.name);
      node.id = normalized.id;
      node.xnaut = normalized;
      for (const input of normalized.inputs) {
        node.addInput(input.id, new classic.Input(socket(input.data_type), input.id, true));
      }
      for (const output of normalized.outputs) {
        node.addOutput(output.id, new classic.Output(socket(output.data_type), output.id, true));
      }
      return node;
    }
    function getNode(id) {
      const node = editor.getNode(id);
      return node ? clone(node.xnaut) : null;
    }
    async function load(definition) {
      loading = true;
      selectedId = "";
      await editor.clear();
      sockets.clear();
      const nodes = /* @__PURE__ */ new Map();
      for (const item of definition.nodes || []) {
        const node = toReteNode(item);
        nodes.set(node.id, node);
        await editor.addNode(node);
        const position = definition.presentation?.nodes?.[node.id] || { x: 80 + nodes.size * 36, y: 80 + nodes.size * 24 };
        await area.translate(node.id, { x: Number(position.x || 0), y: Number(position.y || 0) });
      }
      for (const item of definition.connections || []) {
        const source = nodes.get(item.from_node);
        const target = nodes.get(item.to_node);
        if (!source || !target || !source.outputs[item.from_port] || !target.inputs[item.to_port]) continue;
        const edge = new classic.Connection(source, item.from_port, target, item.to_port);
        edge.id = item.id;
        await editor.addConnection(edge);
      }
      const viewport = definition.presentation || {};
      await area.area.translate(Number(viewport.viewport_x || 0), Number(viewport.viewport_y || 0));
      await area.area.zoom(Number(viewport.zoom || 1), 0, 0);
      if ((definition.nodes || []).length) await index.zoomAt(area, editor.getNodes(), { scale: 0.86 });
      loading = false;
    }
    async function addNode(definition, position) {
      const node = toReteNode(definition);
      await editor.addNode(node);
      const transform = area.area.transform;
      const x2 = position?.x ?? (container.clientWidth / 2 - transform.x) / transform.k - 110;
      const y3 = position?.y ?? (container.clientHeight / 2 - transform.y) / transform.k - 40;
      await area.translate(node.id, { x: x2, y: y3 });
      await selector2.unselectAll();
      selectedId = node.id;
      hooks.onSelect?.(selectedId, clone(node.xnaut));
      notifyChange();
      return clone(node.xnaut);
    }
    async function updateNode(id, definition) {
      const node = editor.getNode(id);
      if (!node) throw new Error(`Node not found: ${id}`);
      node.xnaut = normalizeNode({ ...node.xnaut, ...definition, id });
      node.label = node.xnaut.name;
      await area.update("node", id);
      hooks.onSelect?.(id, clone(node.xnaut));
      notifyChange();
    }
    async function removeSelected() {
      if (!selectedId) return;
      for (const edge of editor.getConnections().filter((item) => item.source === selectedId || item.target === selectedId)) {
        await editor.removeConnection(edge.id);
      }
      await editor.removeNode(selectedId);
      selectedId = "";
      hooks.onSelect?.("", null);
    }
    function serialize(base) {
      const presentationNodes = {};
      const nodes = editor.getNodes().map((node) => {
        const position = area.nodeViews.get(node.id)?.position || { x: 0, y: 0 };
        presentationNodes[node.id] = { x: position.x, y: position.y, collapsed: false };
        return clone(node.xnaut);
      });
      const connections = editor.getConnections().map((item) => ({
        id: item.id,
        from_node: item.source,
        from_port: item.sourceOutput,
        to_node: item.target,
        to_port: item.targetInput
      }));
      return {
        ...clone(base),
        nodes,
        connections,
        presentation: {
          nodes: presentationNodes,
          viewport_x: area.area.transform.x,
          viewport_y: area.area.transform.y,
          zoom: area.area.transform.k
        }
      };
    }
    async function setRun(run) {
      for (const node of editor.getNodes()) {
        node.xnaut.runStatus = run?.nodes?.[node.id]?.status || "pending";
        await area.update("node", node.id);
      }
    }
    async function focus() {
      if (editor.getNodes().length) await index.zoomAt(area, editor.getNodes(), { scale: 0.86 });
    }
    function destroy2() {
      accumulating.destroy();
      area.destroy();
    }
    return { load, addNode, updateNode, removeSelected, serialize, setRun, focus, getNode, destroy: destroy2 };
  }
  return __toCommonJS(loops_editor_entry_exports);
})();
/*! Bundled license information:

@babel/runtime/helpers/regenerator.js:
  (*! regenerator-runtime -- Copyright (c) 2014-present, Facebook, Inc. -- license (MIT): https://github.com/babel/babel/blob/main/packages/babel-helpers/LICENSE *)

@lit/reactive-element/css-tag.js:
  (**
   * @license
   * Copyright 2019 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

@lit/reactive-element/reactive-element.js:
lit-html/lit-html.js:
lit-element/lit-element.js:
@lit/reactive-element/decorators/custom-element.js:
@lit/reactive-element/decorators/property.js:
@lit/reactive-element/decorators/state.js:
@lit/reactive-element/decorators/event-options.js:
@lit/reactive-element/decorators/base.js:
@lit/reactive-element/decorators/query.js:
@lit/reactive-element/decorators/query-all.js:
@lit/reactive-element/decorators/query-async.js:
@lit/reactive-element/decorators/query-assigned-nodes.js:
lit-html/directive.js:
  (**
   * @license
   * Copyright 2017 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

lit-html/is-server.js:
  (**
   * @license
   * Copyright 2022 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

rete/rete.esm.js:
  (*!
  * rete v2.0.5
  * (c) 2025 Vitaliy Stoliarov
  * Released under the MIT license.
  * *)

rete-area-plugin/rete-area-plugin.esm.js:
  (*!
  * rete-area-plugin v2.3.1
  * (c) 2026 Vitaliy Stoliarov
  * Released under the MIT license.
  * *)

rete-connection-plugin/rete-connection-plugin.esm.js:
  (*!
  * rete-connection-plugin v2.0.4
  * (c) 2024 Vitaliy Stoliarov
  * Released under the MIT license.
  * *)

rete-render-utils/rete-render-utils.esm.js:
  (*!
  * rete-render-utils v2.0.2
  * (c) 2024 Vitaliy Stoliarov
  * Released under the MIT license.
  * *)

@lit/reactive-element/decorators/query-assigned-elements.js:
  (**
   * @license
   * Copyright 2021 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

lit-html/directives/style-map.js:
  (**
   * @license
   * Copyright 2018 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

rete-lit-plugin/dist/rete-litv-plugin.esm.js:
  (*!
  * rete-vue-plugin v2.0.3
  * (c) 2023 Vitaliy Stoliarov
  * Released under the MIT license.
  * *)
*/
