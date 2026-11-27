// One fake DOM and one recording canvas context for the chart's Node tests (never copy these into a test).
// The document refuses innerHTML, outerHTML and insertAdjacentHTML outright: feed text must stay text.
"use strict";

// a text's width in chart units, from the pixel size in a CSS font string
const measure = (text, font) => [...text].length * parseFloat(/(\d+(?:\.\d+)?)px/.exec(font)[1]) * 0.55;

function fakeDocument() {
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
      set textContent(v) { el.childNodes = [text(v)]; },
      get textContent() { return el.childNodes.map(n => (n.nodeType === 3 ? n.data : n.textContent)).join(""); },
      set innerHTML(_) { throw new Error("innerHTML must never be used"); },
      set outerHTML(_) { throw new Error("outerHTML must never be used"); },
      insertAdjacentHTML() { throw new Error("insertAdjacentHTML must never be used"); },
    };
    return el;
  };
  return {createElement: element, createTextNode: text};
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

module.exports = {measure, fakeDocument, walk, elements, byClass, recordingContext};
