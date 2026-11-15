//! Stable callbacks shared by keyboard and pointer handlers across local rerenders.
use ratatui_kit::{Handler, Hooks, UseState};
use std::sync::{Arc, Mutex};

pub struct Callback<T>(Arc<Mutex<Handler<'static, T>>>);
impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Callback<T> {
    pub fn call(&self, value: T) {
        (self.0.lock().unwrap_or_else(|e| e.into_inner()))(value);
    }
}
pub fn callback<T: 'static>(f: impl FnMut(T) + Send + Sync + 'static) -> Callback<T> {
    Callback(Arc::new(Mutex::new(Handler::from(f))))
}
pub fn use_callback<T: 'static>(
    hooks: &mut Hooks,
    incoming: &mut Handler<'static, T>,
) -> Callback<T> {
    let shared = hooks
        .use_state(|| Callback(Arc::new(Mutex::new(Handler::default()))))
        .read()
        .clone();
    // A child's local update can reuse props whose handler was already taken.
    if !incoming.is_default() {
        *shared.0.lock().unwrap_or_else(|e| e.into_inner()) = incoming.take();
    }
    shared
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui_kit::prelude::*;
    #[derive(Props)]
    struct CallbackProbeProps {
        on_call: Handler<'static, bool>,
    }

    #[component]
    fn CallbackProbe(
        props: &mut CallbackProbeProps,
        mut hooks: Hooks,
    ) -> impl Into<AnyElement<'static>> {
        let callback = use_callback(&mut hooks, &mut props.on_call);
        let mut frame = hooks.use_state(|| 0);
        hooks.use_effect(
            move || {
                let current = frame.get();
                if current < 3 {
                    callback.call(current > 0);
                    frame.set(current + 1);
                }
            },
            frame.get(),
        );
        element!(Text(text: frame.get().to_string()))
    }

    #[test]
    fn callbacks_survive_local_renders_with_consumed_props() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let result = calls.clone();
        let _ = ratatui_kit::test_util::render_frames(
            element!(CallbackProbe(
                ..CallbackProbeProps {
                    on_call: Handler::from(move |checked| calls.lock().unwrap().push(checked)),
                }
            )),
            10,
            1,
            4,
        );
        assert_eq!(*result.lock().unwrap(), [false, true, true]);
    }
}
