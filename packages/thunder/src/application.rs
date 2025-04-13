use blitz_renderer_vello::BlitzVelloRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

use crate::JsDocument;

pub struct ThunderApplication {
    inner: BlitzApplication<JsDocument, BlitzVelloRenderer>,
}

impl ApplicationHandler<BlitzShellEvent> for ThunderApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.inner.new_events(event_loop, cause);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.inner.window_event(event_loop, window_id, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: BlitzShellEvent) {
        match event {
            BlitzShellEvent::Embedder(event) => {
                let Ok(_event) = event.downcast::<ThunderEvent>() else {
                    return;
                };
            }
            event => self.inner.user_event(event_loop, event),
        }
    }
}

enum ThunderEvent {}
