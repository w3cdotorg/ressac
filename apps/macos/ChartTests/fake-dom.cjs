// One fake DOM and one recording canvas context for the chart's Node tests (never copy these into a test).
// The document refuses innerHTML, outerHTML and insertAdjacentHTML outright: feed text must stay text.
"use strict";

// a text's width in chart units, from the pixel size in a CSS font string
const measure = (text, font) => [...text].length * parseFloat(/(\d+(?:\.\d+)?)px/.exec(font)[1]) * 0.55;

function fakeDocument() {
  const doc = {activeElement: null, body: null}; // focus() sets activeElement; nothing ever leaves it on its own
  const text = data => ({nodeType: 3, data: String(data)});
  const element = tag => {
    const el = {
      nodeType: 1, tagName: tag.toUpperCase(), childNodes: [], attributes: {}, listeners: {}, className: "", style: {},
      rect: {left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0, x: 0, y: 0}, // settable: what getBoundingClientRect returns
      append(...nodes) { nodes.forEach(n => el.childNodes.push(typeof n === "string" ? text(n) : n)); },
      replaceChildren(...nodes) { el.childNodes = []; el.append(...nodes); },
      setAttribute(k, v) { el.attributes[k] = String(v); },
      getAttribute(k) { return el.attributes[k] ?? null; },
      addEventListener(type, fn) { (el.listeners[type] ||= []).push(fn); },
      dispatch(type, event) { (el.listeners[type] || []).forEach(fn => fn(event)); }, // any event type
      getBoundingClientRect() { return el.rect; },
      focus() { doc.activeElement = el; },
      contains(node) { return node === el || el.childNodes.some(c => c.nodeType === 1 && c.contains(node)); },
      // ".class" only: the first element below this one carrying that class
      querySelector(sel) { return elements(el.childNodes).find(e => e.className.split(" ").includes(sel.replace(/^\./, ""))) || null; },
      classList: {
        add: (...c) => { el.className = [...new Set([...el.className.split(" ").filter(Boolean), ...c])].join(" "); },
        remove: (...c) => { el.className = el.className.split(" ").filter(x => x && !c.includes(x)).join(" "); },
        contains: c => el.className.split(" ").includes(c),
        toggle: (c, on) => { const want = on ?? !el.classList.contains(c); want ? el.classList.add(c) : el.classList.remove(c); return want; },
      },
      dataset: {},
      setSelectionRange(start, end) { el.selectionStart = start; el.selectionEnd = end; },
      set textContent(v) { el.childNodes = [text(v)]; },
      get textContent() { return el.childNodes.map(n => (n.nodeType === 3 ? n.data : n.textContent)).join(""); },
      set innerHTML(_) { throw new Error("innerHTML must never be used"); },
      set outerHTML(_) { throw new Error("outerHTML must never be used"); },
      insertAdjacentHTML() { throw new Error("insertAdjacentHTML must never be used"); },
    };
    return el;
  };
  Object.assign(doc, {createElement: element, createTextNode: text});
  doc.body = element("body");
  doc.documentElement = element("html");
  doc.documentElement.style.setProperty = (k, v) => { doc.documentElement.style[k] = v; };
  doc.listeners = {};
  doc.addEventListener = (type, fn) => { (doc.listeners[type] ||= []).push(fn); };
  doc.dispatch = (type, event) => { (doc.listeners[type] || []).forEach(fn => fn(event)); };
  return doc;
}

// A window for createPage: the fake document (canvases draw into recording contexts, kept as canvas.recording), a size, timers and
// animation frames that run only when the test says so, matchMedia answering `reduced` for Reduce Motion, and
// the app's message handler when `bridge` is true (its messages land in win.posted); the page's logged errors land in
// win.errors.
function fakeWindow({width = 1920, height = 1080, bridge = true, reduced = false} = {}) {
  const document = fakeDocument(), make = document.createElement;
  document.createElement = tag => {
    const el = make(tag);
    if (el.tagName === "CANVAS") { const rec = recordingContext(); el.getContext = () => rec.ctx; el.recording = rec; el.width = 0; el.height = 0; }
    return el;
  };
  const timers = [], frames = [], posted = [], errors = [], listeners = {};
  const win = {
    document, innerWidth: width, innerHeight: height, devicePixelRatio: 1, posted, timers, frames, errors,
    console: {error: e => errors.push(e)}, // the page logs what a document throws: tests assert none
    performance: {now: () => 0},
    matchMedia: query => ({matches: /reduced-motion/.test(query) ? reduced : false, addEventListener() {}}),
    requestAnimationFrame: fn => { frames.push(fn); return frames.length; },
    cancelAnimationFrame() {},
    setTimeout: (fn, ms) => { timers.push({fn, ms}); return timers.length; },
    clearTimeout() {},
    addEventListener: (type, fn) => { (listeners[type] ||= []).push(fn); },
  };
  if (bridge) win.webkit = {messageHandlers: {ressac: {postMessage: m => posted.push(m)}}};
  return win;
}
const walk = (nodes, out = []) => { nodes.forEach(n => { out.push(n); if (n.childNodes) walk(n.childNodes, out); }); return out; };
const elements = nodes => walk(nodes).filter(n => n.nodeType === 1);
const byClass = (nodes, cls) => elements(nodes).find(n => n.className.split(" ").includes(cls));

// a canvas context that records what is drawn
function recordingContext() {
  const log = [], state = {};
  const ctx = new Proxy(state, {
    get(target, prop) {
      if (prop in target) return target[prop];
      return (...args) => {
        log.push([prop, ...args]);
        if (prop === "measureText") return {width: measure(String(args[0]), target.font || "10px serif")};
        if (prop === "createRadialGradient" || prop === "createLinearGradient") return {addColorStop() {}};
        return undefined;
      };
    },
    set(target, prop, value) { target[prop] = value; return true; },
  });
  return {ctx, log, texts: () => log.filter(([op]) => op === "fillText").map(([, t]) => t)};
}

module.exports = {measure, fakeDocument, fakeWindow, walk, elements, byClass, recordingContext};
