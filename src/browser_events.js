// Event constructors adapted from main's gpu_ui/input_bootstrap.js.
// This binding has no presenter, video, native input, or host capabilities.
(function installBrowserEvents(G) {
    'use strict';
    const listeners = new WeakMap();
    const optionCapture = (options) =>
        typeof options === 'boolean' ? options : !!(options && options.capture);
    const optionOnce = (options) => !!(options && typeof options === 'object' && options.once);
    const optionPassive = (options) => !!(options && typeof options === 'object' && options.passive);

    class Event {
        constructor(type, init) {
            if (arguments.length === 0) throw new TypeError('Event type is required');
            init = init || {};
            this.type = String(type);
            this.bubbles = !!init.bubbles;
            this.cancelable = !!init.cancelable;
            this.composed = !!init.composed;
            this.defaultPrevented = false;
            this.eventPhase = Event.NONE;
            this.target = null;
            this.currentTarget = null;
            this.isTrusted = false;
            this.timeStamp = G.performance && typeof G.performance.now === 'function'
                ? G.performance.now()
                : Date.now();
            this._dispatching = false;
            this._stopped = false;
            this._immediateStopped = false;
            this._passive = false;
            this._path = [];
            this._delivered = 0;
        }

        preventDefault() {
            if (this.cancelable && !this._passive) this.defaultPrevented = true;
        }
        stopPropagation() { this._stopped = true; }
        stopImmediatePropagation() {
            this._stopped = true;
            this._immediateStopped = true;
        }
        composedPath() { return this._path.slice(); }
        initEvent(type, bubbles, cancelable) {
            if (this._dispatching) return;
            this.type = String(type);
            this.bubbles = !!bubbles;
            this.cancelable = !!cancelable;
        }
    }
    Event.NONE = 0;
    Event.CAPTURING_PHASE = 1;
    Event.AT_TARGET = 2;
    Event.BUBBLING_PHASE = 3;
    Event.prototype.NONE = Event.NONE;
    Event.prototype.CAPTURING_PHASE = Event.CAPTURING_PHASE;
    Event.prototype.AT_TARGET = Event.AT_TARGET;
    Event.prototype.BUBBLING_PHASE = Event.BUBBLING_PHASE;

    class CustomEvent extends Event {
        constructor(type, init) {
            super(type, init);
            this.detail = init && 'detail' in init ? init.detail : null;
        }
        initCustomEvent(type, bubbles, cancelable, detail) {
            this.initEvent(type, bubbles, cancelable);
            this.detail = detail;
        }
    }

    class UIEvent extends Event {
        constructor(type, init) {
            super(type, init);
            init = init || {};
            this.view = init.view || null;
            this.detail = Number(init.detail) || 0;
            this.which = 0;
        }
    }

    class MouseEvent extends UIEvent {
        constructor(type, init) {
            super(type, init);
            init = init || {};
            this.screenX = Number(init.screenX) || 0;
            this.screenY = Number(init.screenY) || 0;
            this.clientX = Number(init.clientX) || 0;
            this.clientY = Number(init.clientY) || 0;
            this.pageX = Number.isFinite(Number(init.pageX)) ? Number(init.pageX) : this.clientX;
            this.pageY = Number.isFinite(Number(init.pageY)) ? Number(init.pageY) : this.clientY;
            this.offsetX = this.clientX;
            this.offsetY = this.clientY;
            this.movementX = Number(init.movementX) || 0;
            this.movementY = Number(init.movementY) || 0;
            this.ctrlKey = !!init.ctrlKey;
            this.shiftKey = !!init.shiftKey;
            this.altKey = !!init.altKey;
            this.metaKey = !!init.metaKey;
            this.button = Number.isFinite(Number(init.button)) ? Number(init.button) : 0;
            this.buttons = Number(init.buttons) >>> 0;
            this.relatedTarget = init.relatedTarget || null;
            this.which = this.button === 0 ? 1 : this.button === 1 ? 2 : this.button === 2 ? 3 : 0;
        }
        getModifierState(key) {
            switch (String(key)) {
                case 'Alt': return this.altKey;
                case 'Control': return this.ctrlKey;
                case 'Meta': return this.metaKey;
                case 'Shift': return this.shiftKey;
                default: return false;
            }
        }
    }

    class WheelEvent extends MouseEvent {
        constructor(type, init) {
            super(type, init);
            init = init || {};
            this.deltaX = Number(init.deltaX) || 0;
            this.deltaY = Number(init.deltaY) || 0;
            this.deltaZ = Number(init.deltaZ) || 0;
            this.deltaMode = Number(init.deltaMode) || WheelEvent.DOM_DELTA_PIXEL;
        }
    }
    WheelEvent.DOM_DELTA_PIXEL = 0;
    WheelEvent.DOM_DELTA_LINE = 1;
    WheelEvent.DOM_DELTA_PAGE = 2;
    WheelEvent.prototype.DOM_DELTA_PIXEL = WheelEvent.DOM_DELTA_PIXEL;
    WheelEvent.prototype.DOM_DELTA_LINE = WheelEvent.DOM_DELTA_LINE;
    WheelEvent.prototype.DOM_DELTA_PAGE = WheelEvent.DOM_DELTA_PAGE;


    const listenerMap = target => {
        let map = listeners.get(target);
        if (!map) { map = new Map(); listeners.set(target, map); }
        return map;
    };
    let listenerCount = 0;
    const remove = (target, type, callback, options) => {
        const map = listenerMap(target), capture = optionCapture(options);
        const entries = map.get(String(type)) || [];
        const kept = entries.filter(e => e.callback !== callback || e.capture !== capture);
        listenerCount -= entries.length - kept.length;
        if (kept.length) map.set(String(type), kept); else map.delete(String(type));
    };
    function invoke(target, event, capture, phase) {
        event.currentTarget = target; event.eventPhase = phase;
        const entries = (listenerMap(target).get(event.type) || []).slice();
        for (const entry of entries) {
            if (event._immediateStopped) break;
            if (entry.capture !== capture) continue;
            // A listener removed during dispatch must not be called from a stale snapshot.
            if (!(listenerMap(target).get(event.type) || []).includes(entry)) continue;
            if (entry.once) remove(target, event.type, entry.callback, entry.capture);
            event._passive = entry.passive;
            try {
                if (typeof entry.callback === 'function') entry.callback.call(target, event);
                else if (typeof entry.callback.handleEvent === 'function') entry.callback.handleEvent(event);
            } catch (error) {
                G.__solaraLastInputError = String(error);
            } finally { event._passive = false; }
        }
        if (!capture && !event._immediateStopped) {
            const handler = target['on' + event.type];
            if (typeof handler === 'function') {
                try { if (handler.call(target, event) === false) event.preventDefault(); }
                catch (error) { G.__solaraLastInputError = String(error); }
            }
        }
    }
    class EventTarget {
        addEventListener(type, callback, options) {
            if (callback === null || callback === undefined) return;
            if (typeof callback !== 'function' && typeof callback !== 'object') return;
            type = String(type);
            const capture = optionCapture(options), map = listenerMap(this);
            const entries = map.get(type) || [];
            if (entries.some(e => e.callback === callback && e.capture === capture)) return;
            if (listenerCount >= 4096) throw Error('Page event listener limit reached');
            entries.push({callback, capture, once: optionOnce(options), passive: optionPassive(options)});
            listenerCount++; map.set(type, entries);
        }
        removeEventListener(type, callback, options) { remove(this, type, callback, options); }
        dispatchEvent(event) {
            if (!(event instanceof Event) || !event.type || event._dispatching) throw new TypeError('Invalid event dispatch');
            const path = [this], seen = new Set(path);
            for (let target = this; typeof target._eventParent === 'function';) {
                target = target._eventParent();
                if (!target) break;
                if (seen.has(target) || path.length >= 32768) throw Error('Invalid event path');
                seen.add(target); path.push(target);
            }
            event.target = this; event._path = path; event._dispatching = true;
            event._stopped = false; event._immediateStopped = false;
            try {
                for (let i = path.length - 1; i > 0 && !event._stopped; i--) invoke(path[i], event, true, Event.CAPTURING_PHASE);
                if (!event._stopped) {
                    invoke(this, event, true, Event.AT_TARGET);
                    if (!event._immediateStopped) invoke(this, event, false, Event.AT_TARGET);
                }
                if (event.bubbles) for (let i = 1; i < path.length && !event._stopped; i++) invoke(path[i], event, false, Event.BUBBLING_PHASE);
            } finally {
                event._dispatching = false; event.currentTarget = null; event.eventPhase = Event.NONE;
                event._path = [];
            }
            return !event.defaultPrevented;
        }
    }
    Object.assign(G, {Event, CustomEvent, UIEvent, MouseEvent, WheelEvent, EventTarget});
})(globalThis);
