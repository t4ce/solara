// A bounded DOM binding for classic page scripts. CSS/layout stay in Blitz.
(() => {
  'use strict';
  const hostRecord = globalThis.__pageRecord, fragment = globalThis.__pageFragment;
  let journal = [];
  function record(command) {
    if (journal.length >= 65536) throw Error('DOM mutation slice limit reached');
    journal.push(command);
  }
  const request = globalThis.__pageFetch;
  delete globalThis.__pageRecord; delete globalThis.__pageFragment; delete globalThis.__pageFetch;
  const nodes = new Map(), pending = new Map(), timers = new Map();
  let serial = 0, requestId = 0, timerId = 0, clock = 0;
  class Event {
    constructor(type, options = {}) { this.type = type; this.bubbles = !!options.bubbles; this.cancelable = !!options.cancelable; this.defaultPrevented = false; }
    preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
    stopPropagation() { this.stopped = true; }
  }
  class EventTarget {
    constructor() { this.listeners = new Map(); }
    addEventListener(type, callback, options = {}) {
      if (!callback) return;
      const list = this.listeners.get(type) || [];
      if (!list.some(x => x.callback === callback)) list.push({ callback, once: !!options.once });
      this.listeners.set(type, list);
    }
    removeEventListener(type, callback) {
      this.listeners.set(type, (this.listeners.get(type) || []).filter(x => x.callback !== callback));
    }
    dispatchEvent(event) {
      event.target = this;
      for (let target = this; target; target = target.parentNode || (target === document ? windowEvents : null)) {
        event.currentTarget = target;
        const property = target['on' + event.type];
        if (typeof property === 'function') property.call(target, event);
        for (const entry of [...(target.listeners.get(event.type) || [])]) {
          if (entry.once) target.removeEventListener(event.type, entry.callback);
          if (typeof entry.callback === 'function') entry.callback.call(target, event);
          else entry.callback.handleEvent(event);
        }
        if (!event.bubbles || event.stopped) break;
      }
      return !event.defaultPrevented;
    }
  }
  class Node extends EventTarget {
    constructor(kind, name, data, id, namespace) {
      super();
      if (nodes.size >= 32768) throw Error('Page node limit reached');
      this._id = id || 'new:' + (++serial); this.nodeType = kind;
      this.nodeName = name; this.namespaceURI = namespace || 'http://www.w3.org/1999/xhtml';
      this._data = data || ''; this._attrs = new Map(); this.childNodes = []; this.parentNode = null;
      nodes.set(this._id, this);
      if (!id) record(['create', this._id, kind, name, this._data, this.namespaceURI]);
    }
    get ownerDocument() { return this.nodeType === 9 ? null : document; }
    get tagName() { return this.nodeType === 1 ? this.nodeName.toUpperCase() : undefined; }
    get children() { return this.childNodes.filter(n => n.nodeType === 1); }
    get parentElement() { return this.parentNode?.nodeType === 1 ? this.parentNode : null; }
    get firstChild() { return this.childNodes[0] || null; }
    get lastChild() { return this.childNodes[this.childNodes.length - 1] || null; }
    get nextSibling() { const list = this.parentNode?.childNodes || []; return list[list.indexOf(this) + 1] || null; }
    contains(other) { for (let n = other; n; n = n.parentNode) if (n === this) return true; return false; }
    appendChild(child) { return this.insertBefore(child, null); }
    insertBefore(child, anchor) {
      if (!(child instanceof Node) || child.contains(this) || (anchor && anchor.parentNode !== this)) throw Error('Invalid DOM insertion');
      if (child === anchor) return child;
      if (child.parentNode) { const old = child.parentNode.childNodes; old.splice(old.indexOf(child), 1); }
      child.parentNode = this;
      this.childNodes.splice(anchor ? this.childNodes.indexOf(anchor) : this.childNodes.length, 0, child);
      record(['insert', this._id, child._id, anchor?._id || null]);
      return child;
    }
    removeChild(child) {
      if (child.parentNode !== this) throw Error('Not a child');
      this.childNodes.splice(this.childNodes.indexOf(child), 1); child.parentNode = null;
      record(['remove', child._id]); return child;
    }
    remove() { this.parentNode?.removeChild(this); }
    replaceChildren(...children) { for (const c of [...this.childNodes]) this.removeChild(c); this.append(...children); }
    append(...children) { for (const c of children) this.appendChild(c instanceof Node ? c : document.createTextNode(String(c))); }
    get textContent() { return this.nodeType === 3 || this.nodeType === 8 ? this._data : this.childNodes.filter(n => n.nodeType !== 8).map(n => n.textContent).join(''); }
    set textContent(value) {
      value = String(value ?? '');
      if (this.nodeType === 3 || this.nodeType === 8) { this._data = value; record(['text', this._id, value]); }
      else this.replaceChildren(...(value ? [document.createTextNode(value)] : []));
    }
    get nodeValue() { return this.nodeType === 3 || this.nodeType === 8 ? this._data : null; }
    set nodeValue(value) { if (this.nodeType === 3 || this.nodeType === 8) this.textContent = value; }
    getAttribute(name) { return this._attrs.get(String(name).toLowerCase()) ?? null; }
    hasAttribute(name) { return this.getAttribute(name) !== null; }
    setAttribute(name, value) {
      name = String(name).toLowerCase(); value = String(value);
      if (!name || /[\s\0"'>/=]/.test(name)) throw Error('Invalid attribute');
      this._attrs.set(name, value); record(['attr', this._id, name, value]);
    }
    removeAttribute(name) { name = String(name).toLowerCase(); this._attrs.delete(name); record(['attr', this._id, name, null]); }
    get classList() {
      const node = this, list = () => (node.getAttribute('class') || '').split(/\s+/).filter(Boolean);
      return {
        contains: value => list().includes(value),
        add(...values) { node.setAttribute('class', [...new Set([...list(), ...values])].join(' ')); },
        remove(...values) { node.setAttribute('class', list().filter(v => !values.includes(v)).join(' ')); },
        toggle(value, force) { const yes = force === undefined ? !this.contains(value) : !!force; yes ? this.add(value) : this.remove(value); return yes; }
      };
    }
    get dataset() {
      const attr = key => 'data-' + String(key).replace(/[A-Z]/g, c => '-' + c.toLowerCase());
      return new Proxy({}, { get: (_, key) => this.getAttribute(attr(key)) ?? undefined, set: (_, key, value) => { this.setAttribute(attr(key), value); return true; } });
    }
    get innerHTML() { return this.childNodes.map(serialize).join(''); }
    set innerHTML(html) {
      const tree = fragment(String(html), this.nodeName.toLowerCase(), this.namespaceURI);
      this.replaceChildren();
      for (const raw of tree.children || []) this.appendChild(build(raw));
    }
    // Simple compound selectors for this initial binding. Unsupported selectors fail explicitly.
    matches(selector) {
      return String(selector).split(',').some(part => {
        let rest = part.trim(), ok = this.nodeType === 1;
        const tag = rest.match(/^(\*|[\w-]+)/);
        if (tag) { ok &&= tag[0] === '*' || this.nodeName.toLowerCase() === tag[0].toLowerCase(); rest = rest.slice(tag[0].length); }
        while (rest) {
          const token = rest.match(/^([.#])([\w-]+)|^\[([\w-]+)(?:=["']?([^"'\]]*)["']?)?\]/);
          if (!token) throw Error('Unsupported DOM selector: ' + selector);
          if (token[1]) ok &&= token[1] === '.' ? this.classList.contains(token[2]) : this.id === token[2];
          else ok &&= token[4] === undefined ? this.hasAttribute(token[3]) : this.getAttribute(token[3]) === token[4];
          rest = rest.slice(token[0].length);
        }
        return ok;
      });
    }
    querySelectorAll(selector) { const out = []; const walk = n => { for (const c of n.childNodes) { if (c.nodeType === 1 && c.matches(selector)) out.push(c); walk(c); } }; walk(this); return out; }
    querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
    closest(selector) { for (let n = this; n; n = n.parentElement) if (n.matches(selector)) return n; return null; }
    focus() { document.activeElement = this; }
    click() { this.dispatchEvent(new Event('click', { bubbles: true, cancelable: true })); }
  }
  for (const [property, attr] of [['id','id'], ['className','class'], ['type','type'], ['value','value'], ['title','title'], ['tabIndex','tabindex']]) {
    Object.defineProperty(Node.prototype, property, { get() { return this.getAttribute(attr) || ''; }, set(v) { this.setAttribute(attr, v); } });
  }
  for (const name of ['hidden', 'disabled', 'checked']) Object.defineProperty(Node.prototype, name, {
    get() { return this.hasAttribute(name); }, set(v) { v ? this.setAttribute(name, '') : this.removeAttribute(name); }
  });
  function build(raw, path) {
    const kind = raw.tagName ? 1 : raw.nodeName === '#text' ? 3 : raw.nodeName === '#document' ? 9 : 8;
    const n = new Node(kind, raw.tagName || raw.nodeName, raw.value || raw.data, path, raw.namespaceURI);
    for (const a of raw.attrs || []) path ? n._attrs.set(a.name, a.value) : n.setAttribute(a.name, a.value);
    (raw.children || []).forEach((child, i) => {
      if (child.nodeName === '#documentType') return;
      const c = build(child, path ? path + '.' + i : undefined);
      if (path) { n.childNodes.push(c); c.parentNode = n; } else n.appendChild(c);
    });
    return n;
  }
  const escape = s => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;');
  function serialize(n) {
    if (n.nodeType === 3) return escape(n._data);
    if (n.nodeType === 8) return '<!--' + n._data + '-->';
    return '<' + n.nodeName + [...n._attrs].map(([k,v]) => ' ' + k + '="' + escape(v) + '"').join('') + '>' + n.innerHTML + '</' + n.nodeName + '>';
  }
  const windowEvents = new EventTarget();
  const document = build(globalThis.__pageTree, 'root'); delete globalThis.__pageTree;
  document.getElementById = id => { const walk = n => { if (n.id === String(id)) return n; for (const c of n.childNodes) { const found = walk(c); if (found) return found; } return null; }; return walk(document); };
  document.createElement = tag => new Node(1, String(tag).toLowerCase());
  document.createElementNS = (ns, tag) => new Node(1, String(tag), '', undefined, ns);
  document.createTextNode = text => new Node(3, '#text', String(text));
  document.createComment = text => new Node(8, '#comment', String(text));
  document.documentElement = document.children[0]; document.head = document.querySelector('head'); document.body = document.querySelector('body');
  document.activeElement = document.body; document.readyState = 'loading';
  globalThis.document = document; globalThis.window = globalThis; globalThis.self = globalThis;
  globalThis.Node = Node; globalThis.Element = Node; globalThis.HTMLElement = Node; globalThis.Event = Event; globalThis.EventTarget = EventTarget;
  globalThis.addEventListener = windowEvents.addEventListener.bind(windowEvents);
  globalThis.removeEventListener = windowEvents.removeEventListener.bind(windowEvents);
  globalThis.dispatchEvent = windowEvents.dispatchEvent.bind(windowEvents);
  globalThis.console = { log() {}, warn() {}, error() {}, info() {} };
  globalThis.fetch = (url, options = {}) => new Promise((resolve, reject) => {
    if (pending.size >= 16) throw Error('Too many pending fetches');
    const id = ++requestId;
    request(id, String(url), options);
    pending.set(id, {resolve, reject});
  });
  globalThis.setTimeout = (callback, delay = 0, ...args) => {
    if (typeof callback !== 'function' || timers.size >= 256) throw Error('Invalid or excessive timer');
    const id = ++timerId; timers.set(id, { callback, args, time: clock + Math.max(0, Number(delay) || 0) }); return id;
  };
  globalThis.clearTimeout = id => timers.delete(id);
  Object.defineProperty(globalThis, '__solara', {value: Object.freeze({
    flush() { if (journal.length) { hostRecord(journal); journal = []; } },
    response(id, body, error) {
      const p = pending.get(id); if (!p) return; pending.delete(id);
      if (error) { p.reject(new TypeError(error)); return; }
      // Native transport currently reports successful bodies, not status metadata.
      p.resolve({ok:true, text:() => Promise.resolve(body), json:() => Promise.resolve().then(() => JSON.parse(body))});
    },
    ready() { document.readyState = 'interactive'; document.dispatchEvent(new Event('DOMContentLoaded', {bubbles:true})); document.readyState = 'complete'; windowEvents.dispatchEvent(new Event('load')); },
    tick(now) { clock = now; for (const [id, timer] of [...timers]) if (timer.time <= now) { timers.delete(id); timer.callback(...timer.args); } },
    click(id) { nodes.get(id)?.click(); }
  })});
})();
