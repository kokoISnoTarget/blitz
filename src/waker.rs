use dioxus::prelude::VirtualDom;
use futures_util::{pin_mut, task::ArcWake, FutureExt};
use std::sync::Arc;
use tao::{event_loop::EventLoopProxy, window::WindowId};

#[derive(Debug, Clone)]
pub struct UserWindowEvent(pub EventData, pub WindowId);

#[derive(Debug, Clone)]
pub enum EventData {
    Poll,

    NewWindow,

    CloseWindow,
}

/// Create a waker that will send a poll event to the event loop.
///
/// This lets the VirtualDom "come up for air" and process events while the main thread is blocked by the WebView.
///
/// All other IO lives in the Tokio runtime,
pub fn tao_waker(proxy: &EventLoopProxy<UserWindowEvent>, id: WindowId) -> std::task::Waker {
    struct DomHandle {
        proxy: EventLoopProxy<UserWindowEvent>,
        id: WindowId,
    }

    // this should be implemented by most platforms, but ios is missing this until
    // https://github.com/tauri-apps/wry/issues/830 is resolved
    unsafe impl Send for DomHandle {}
    unsafe impl Sync for DomHandle {}

    impl ArcWake for DomHandle {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            _ = arc_self
                .proxy
                .send_event(UserWindowEvent(EventData::Poll, arc_self.id));
        }
    }

    futures_util::task::waker(Arc::new(DomHandle {
        id,
        proxy: proxy.clone(),
    }))
}

pub struct PolledVirtualdom {
    dom: VirtualDom,
    waker: std::task::Waker,
}

/// Poll the virtualdom until it's pending
///
/// The waker we give it is connected to the event loop, so it will wake up the event loop when it's ready to be polled again
///
/// All IO is done on the tokio runtime we started earlier
fn poll_vdom(view: &mut PolledVirtualdom) {
    // Build the waker which we'll hand off
    let mut cx = std::task::Context::from_waker(&view.waker);

    loop {
        let fut = view.dom.wait_for_work();
        pin_mut!(fut);

        match fut.poll_unpin(&mut cx) {
            std::task::Poll::Ready(_) => {}
            std::task::Poll::Pending => break,
        }

        // send_edits(view.dom.render_immediate(), &view.desktop_context.webview);
    }
}
