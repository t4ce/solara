(function installSolaraInputHost(G) {
    'use strict';
    if (G.__solaraInputHostVersion === 1) return;

    const listeners = Symbol('solara.listeners');
    const facadeTarget = Symbol('solara.facadeTarget');

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

    const listenerMap = (target) => {
        if (!Object.prototype.hasOwnProperty.call(target, listeners)) {
            Object.defineProperty(target, listeners, { value: new Map() });
        }
        return target[listeners];
    };

    const reportListenerError = (error) => {
        G.__solaraLastInputError = String(error && error.stack ? error.stack : error);
    };

    const invoke = (store, publicTarget, event, phase) => {
        if (!(event instanceof Event)) throw new TypeError('dispatchEvent requires an Event');
        if (event._dispatching) throw new Error('Event is already being dispatched');
        if (event.target === null) event.target = publicTarget;
        event.currentTarget = publicTarget;
        event.eventPhase = phase;
        event._dispatching = true;
        event._immediateStopped = false;
        const entries = (listenerMap(store).get(event.type) || []).slice();
        try {
            for (const entry of entries) {
                if (event._immediateStopped) break;
                if (entry.once) remove(store, event.type, entry.callback, entry.capture);
                event._passive = entry.passive;
                try {
                    if (typeof entry.callback === 'function') {
                        entry.callback.call(publicTarget, event);
                    } else if (entry.callback && typeof entry.callback.handleEvent === 'function') {
                        entry.callback.handleEvent(event);
                    }
                    event._delivered += 1;
                } catch (error) {
                    reportListenerError(error);
                }
            }
            const handler = publicTarget['on' + event.type];
            if (!event._immediateStopped && typeof handler === 'function') {
                try {
                    if (handler.call(publicTarget, event) === false) event.preventDefault();
                    event._delivered += 1;
                } catch (error) {
                    reportListenerError(error);
                }
            }
        } finally {
            event._passive = false;
            event._dispatching = false;
            event.currentTarget = null;
            event.eventPhase = Event.NONE;
        }
        return !event.defaultPrevented;
    };

    const add = (store, type, callback, options) => {
        if (callback === null || callback === undefined) return;
        type = String(type);
        const capture = optionCapture(options);
        const entries = listenerMap(store).get(type) || [];
        if (entries.some((entry) => entry.callback === callback && entry.capture === capture)) return;
        entries.push({
            callback,
            capture,
            once: optionOnce(options),
            passive: optionPassive(options),
        });
        listenerMap(store).set(type, entries);
    };

    const remove = (store, type, callback, options) => {
        type = String(type);
        const capture = typeof options === 'boolean' ? options : optionCapture(options);
        const entries = listenerMap(store).get(type);
        if (!entries) return;
        const kept = entries.filter((entry) => entry.callback !== callback || entry.capture !== capture);
        if (kept.length === 0) listenerMap(store).delete(type);
        else listenerMap(store).set(type, kept);
    };

    class EventTarget {
        constructor() { listenerMap(this); }
        addEventListener(type, callback, options) { add(this, type, callback, options); }
        removeEventListener(type, callback, options) { remove(this, type, callback, options); }
        dispatchEvent(event) { return invoke(this, this, event, Event.AT_TARGET); }
    }

    const installFacade = (object) => {
        const store = new EventTarget();
        Object.defineProperty(object, facadeTarget, { configurable: true, value: store });
        object.addEventListener = (type, callback, options) => add(store, type, callback, options);
        object.removeEventListener = (type, callback, options) => remove(store, type, callback, options);
        object.dispatchEvent = (event) => invoke(store, object, event, Event.AT_TARGET);
        return store;
    };

    const documentObject = G.document && typeof G.document === 'object' ? G.document : {};
    const documentEvents = installFacade(documentObject);
    const windowEvents = installFacade(G);
    G.window = G;
    G.self = G;
    G.top = G;
    G.parent = G;
    G.document = documentObject;
    G.Event = Event;
    G.CustomEvent = CustomEvent;
    G.UIEvent = UIEvent;
    G.MouseEvent = MouseEvent;
    G.WheelEvent = WheelEvent;
    G.EventTarget = EventTarget;

    const viewportState = { x: 0, y: 0, scale: 1 };
    const viewportOffset = (axis) => ({
        configurable: true,
        enumerable: true,
        get: () => viewportState[axis],
    });
    Object.defineProperties(G, {
        scrollX: viewportOffset('x'),
        scrollY: viewportOffset('y'),
        pageXOffset: viewportOffset('x'),
        pageYOffset: viewportOffset('y'),
        devicePixelRatio: viewportOffset('scale'),
    });
    G.visualViewport = Object.freeze({
        get offsetLeft() { return viewportState.x; },
        get offsetTop() { return viewportState.y; },
        get pageLeft() { return viewportState.x; },
        get pageTop() { return viewportState.y; },
        get scale() { return viewportState.scale; },
    });
    G.__solaraSetViewport = (x, y, zoomPercent) => {
        viewportState.x = Math.max(0, Number(x) || 0);
        viewportState.y = Math.max(0, Number(y) || 0);
        viewportState.scale = Math.max(0.1, Math.min(5, (Number(zoomPercent) || 100) / 100));
        return { x: viewportState.x, y: viewportState.y, scale: viewportState.scale };
    };

    G.__solaraDispatchMouse = (payload) => {
        payload = payload || {};
        const type = String(payload.type || 'mousemove');
        const init = {
            bubbles: true,
            cancelable: true,
            composed: true,
            view: G,
            screenX: payload.screenX,
            screenY: payload.screenY,
            clientX: payload.clientX,
            clientY: payload.clientY,
            pageX: payload.pageX,
            pageY: payload.pageY,
            movementX: payload.movementX,
            movementY: payload.movementY,
            button: payload.button,
            buttons: payload.buttons,
            ctrlKey: payload.ctrlKey,
            shiftKey: payload.shiftKey,
            altKey: payload.altKey,
            metaKey: payload.metaKey,
            deltaX: payload.deltaX,
            deltaY: payload.deltaY,
            deltaZ: 0,
            deltaMode: WheelEvent.DOM_DELTA_PIXEL,
        };
        const event = type === 'wheel' ? new WheelEvent(type, init) : new MouseEvent(type, init);
        event.isTrusted = true;
        event._path = [documentObject, G];
        invoke(documentEvents, documentObject, event, Event.AT_TARGET);
        if (event.bubbles && !event._stopped) {
            invoke(windowEvents, G, event, Event.BUBBLING_PHASE);
        }
        return {
            defaultPrevented: event.defaultPrevented,
            delivered: event._delivered,
            type: event.type,
        };
    };

    Object.defineProperty(G, '__solaraInputHostVersion', { value: 1 });
})(globalThis);
