use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use blitz_dom::net::Resource;
use blitz_renderer_vello::BlitzVelloRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent};
use blitz_traits::net::SharedCallback;
use bytes::Bytes;
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::WindowId,
};

use crate::{
    JsDocument, fetch_thread::ScriptOptions, module::ModuleId, v8intergration::IsolateExt,
};

pub type Inner = BlitzApplication<JsDocument, BlitzVelloRenderer>;

pub struct ThunderApplication {
    inner: Inner,
}
impl ThunderApplication {
    pub fn new(proxy: EventLoopProxy<BlitzShellEvent>) -> ThunderApplication {
        let inner = Inner::new(proxy);
        ThunderApplication { inner }
    }
}

impl ApplicationHandler<BlitzShellEvent> for ThunderApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.resumed(event_loop);
        self.inner.windows.iter_mut().for_each(|(window_id, view)| {
            view.doc.resume(window_id);
        });
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
                let Ok(event) = event.downcast::<ThunderEvent>() else {
                    #[cfg(feature = "tracing")]
                    tracing::error!("Could not cast embedder event to ThunderEvent");
                    return;
                };
                if let Some(ref window) = event.window {
                    let Some(view) = self.windows.get_mut(window) else {
                        #[cfg(feature = "tracing")]
                        tracing::error!("Could not find window for embedder event");
                        return;
                    };
                    view.doc.thunder_event(&event.ty);
                    if matches!(event.ty, ThunderEventType::RepollParser) {
                        view.request_redraw();
                    }
                } else if let Some((_window_id, view)) = self.windows.iter_mut().next() {
                    view.doc.thunder_event(&event.ty);
                } else if let Some(doc) = self
                    .pending_windows
                    .first_mut()
                    .map(|pending| &mut pending.doc)
                {
                    doc.thunder_event(&event.ty);
                }
            }
            event => self.inner.user_event(event_loop, event),
        }
    }
}

impl Deref for ThunderApplication {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl DerefMut for ThunderApplication {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ThunderEventType {
    DocumentFetched {
        url: String,
        bytes: Bytes,
    },
    ScriptFetched {
        content: Bytes,
        options: Box<ScriptOptions>,
    },
    ModuleFetched {
        parent_id: Option<ModuleId>,
        module_id: ModuleId,
        content: Bytes,
        options: Box<ScriptOptions>,
    },
    RepollParser,
}

#[derive(Debug)]
pub(crate) struct ThunderEvent {
    window: Option<WindowId>,
    ty: ThunderEventType,
}

#[derive(Clone)]
pub(crate) struct EventProxy(EventLoopProxy<BlitzShellEvent>, Option<WindowId>);
impl EventProxy {
    pub fn new(proxy: EventLoopProxy<BlitzShellEvent>) -> EventProxy {
        EventProxy(proxy, None)
    }
    pub(crate) fn net_callback(&self) -> SharedCallback<Resource> {
        blitz_shell::BlitzShellNetCallback::shared(self.0.clone())
    }
    pub fn set_window(&mut self, window_id: WindowId) {
        self.1 = Some(window_id)
    }
    pub fn repoll_parser(&self) {
        self.0
            .send_event(BlitzShellEvent::embedder_event(ThunderEvent {
                window: self.1,
                ty: ThunderEventType::RepollParser,
            }))
            .unwrap();
    }
    pub fn fetched_script(&self, content: Bytes, options: Box<ScriptOptions>) {
        self.0
            .send_event(BlitzShellEvent::embedder_event(ThunderEvent {
                window: self.1,
                ty: ThunderEventType::ScriptFetched { content, options },
            }))
            .unwrap();
    }
    pub fn fetched_document(&self, url: String, bytes: Bytes) {
        self.0
            .send_event(BlitzShellEvent::embedder_event(ThunderEvent {
                window: self.1,
                ty: ThunderEventType::DocumentFetched { url, bytes },
            }))
            .unwrap();
    }
}
