use rust_qjs_dom::JsEngine;
use serde_json::json;

const INPUT_BOOTSTRAP: &str = include_str!("input_bootstrap.js");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MouseEventKind {
    Move,
    Down,
    Up,
    Click,
    Wheel,
}

impl MouseEventKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Move => "mousemove",
            Self::Down => "mousedown",
            Self::Up => "mouseup",
            Self::Click => "click",
            Self::Wheel => "wheel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MouseInput {
    pub kind: MouseEventKind,
    pub client_x: f32,
    pub client_y: f32,
    pub screen_x: f32,
    pub screen_y: f32,
    pub movement_x: f32,
    pub movement_y: f32,
    pub button: i16,
    pub buttons: u32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
}

impl MouseInput {
    pub(crate) const fn at(kind: MouseEventKind, x: f32, y: f32, buttons: u32) -> Self {
        Self {
            kind,
            client_x: x,
            client_y: y,
            screen_x: x,
            screen_y: y,
            movement_x: 0.0,
            movement_y: 0.0,
            button: 0,
            buttons,
            delta_x: 0.0,
            delta_y: 0.0,
            ctrl_key: false,
            shift_key: false,
            alt_key: false,
            meta_key: false,
        }
    }

    pub(crate) const fn with_screen(mut self, x: f32, y: f32) -> Self {
        self.screen_x = x;
        self.screen_y = y;
        self
    }

    pub(crate) const fn with_movement(mut self, x: f32, y: f32) -> Self {
        self.movement_x = x;
        self.movement_y = y;
        self
    }

    pub(crate) const fn with_button(mut self, button: i16) -> Self {
        self.button = button;
        self
    }

    pub(crate) const fn with_wheel(mut self, x: f32, y: f32) -> Self {
        self.delta_x = x;
        self.delta_y = y;
        self
    }

    pub(crate) const fn with_modifiers(
        mut self,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    ) -> Self {
        self.ctrl_key = ctrl;
        self.shift_key = shift;
        self.alt_key = alt;
        self.meta_key = meta;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MouseDispatch {
    pub default_prevented: bool,
    pub delivered: u64,
}

pub(crate) fn install(js: &mut JsEngine) -> Result<(), String> {
    js.eval_void(INPUT_BOOTSTRAP, "<solara-input-bootstrap>")
        .map_err(|error| format!("failed to initialize Solara mouse events: {error}"))
}

pub(crate) fn dispatch(js: &mut JsEngine, input: MouseInput) -> Result<MouseDispatch, String> {
    let payload = json!({
        "type": input.kind.as_str(),
        "clientX": finite(input.client_x),
        "clientY": finite(input.client_y),
        "screenX": finite(input.screen_x),
        "screenY": finite(input.screen_y),
        "movementX": finite(input.movement_x),
        "movementY": finite(input.movement_y),
        "button": input.button,
        "buttons": input.buttons,
        "deltaX": finite(input.delta_x),
        "deltaY": finite(input.delta_y),
        "ctrlKey": input.ctrl_key,
        "shiftKey": input.shift_key,
        "altKey": input.alt_key,
        "metaKey": input.meta_key,
    });
    let result = js
        .call_global_json("__solaraDispatchMouse", &[payload])
        .map_err(|error| format!("failed to dispatch Solara mouse event: {error}"))?;
    Ok(MouseDispatch {
        default_prevented: result["defaultPrevented"].as_bool().unwrap_or(false),
        delivered: result["delivered"].as_u64().unwrap_or(0),
    })
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use rust_qjs_dom::JsEngine;

    use super::{MouseEventKind, MouseInput, dispatch, install};

    #[test]
    fn native_packet_reaches_document_and_window_as_a_mouse_event() {
        let mut js = JsEngine::new().expect("QuickJS starts");
        install(&mut js).expect("mouse host installs");
        js.eval_void(
            r#"
            globalThis.receivedMouse = [];
            document.addEventListener('mousemove', event => receivedMouse.push({
                target: event.target === document,
                current: event.currentTarget === document,
                ctor: event instanceof MouseEvent,
                trusted: event.isTrusted,
                x: event.clientX,
                y: event.clientY,
                dx: event.movementX,
                dy: event.movementY,
                buttons: event.buttons,
            }));
            window.addEventListener('mousemove', event => receivedMouse.push({
                target: event.target === document,
                current: event.currentTarget === window,
                phase: event.eventPhase,
            }));
            "#,
            "mouse-listeners.js",
        )
        .expect("listeners register");

        let outcome = dispatch(
            &mut js,
            MouseInput::at(MouseEventKind::Move, 120.0, 80.0, 1).with_movement(4.0, -3.0),
        )
        .expect("mouse dispatch succeeds");
        assert_eq!(outcome.delivered, 2);
        assert_eq!(
            js.eval_json("receivedMouse", "mouse-result.js")
                .expect("result serializes"),
            serde_json::json!([
                {
                    "target": true,
                    "current": true,
                    "ctor": true,
                    "trusted": true,
                    "x": 120,
                    "y": 80,
                    "dx": 4,
                    "dy": -3,
                    "buttons": 1,
                },
                {
                    "target": true,
                    "current": true,
                    "phase": 3,
                }
            ])
        );
    }

    #[test]
    fn prevent_default_crosses_back_to_native_code() {
        let mut js = JsEngine::new().expect("QuickJS starts");
        install(&mut js).expect("mouse host installs");
        js.eval_void(
            "document.addEventListener('click', event => event.preventDefault());",
            "prevent-click.js",
        )
        .expect("listener registers");
        let outcome = dispatch(&mut js, MouseInput::at(MouseEventKind::Click, 1.0, 2.0, 0))
            .expect("click dispatch succeeds");
        assert!(outcome.default_prevented);
        assert_eq!(outcome.delivered, 1);
    }
}
