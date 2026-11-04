use futures::{stream::BoxStream, task::ArcWake, FutureExt, StreamExt};
use iocraft::{prelude::*, Canvas};
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
};

#[component]
fn BecomesIdle(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut rendered = hooks.use_state(|| false);
    let mut received_key = hooks.use_state(|| false);
    hooks.use_terminal_events(move |event| {
        if matches!(event, TerminalEvent::Key(_)) {
            received_key.set(true);
        }
    });
    if !rendered.get() {
        rendered.set(true);
    }
    if received_key.get() {
        hooks.use_context_mut::<SystemContext>().exit();
    }
    element!(Text(content: if received_key.get() { "key" } else { "idle" }))
}

#[derive(Default)]
struct PendingInput {
    waker: Option<Waker>,
    event: Option<TerminalEvent>,
}

struct PendingReaderBackend {
    input: Arc<Mutex<PendingInput>>,
    frames: Arc<Mutex<Vec<String>>>,
}

impl TerminalBackend for PendingReaderBackend {
    fn size(&self) -> Option<(u16, u16)> {
        Some((80, 25))
    }

    fn clear_canvas(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write_canvas(&mut self, _: Option<&Canvas>, canvas: &Canvas) -> io::Result<()> {
        self.frames.lock().unwrap().push(canvas.to_string());
        Ok(())
    }

    fn print_above(&mut self, _: &[Passthrough<'_>]) -> io::Result<()> {
        Ok(())
    }

    fn event_stream(&mut self) -> io::Result<BoxStream<'static, io::Result<TerminalEvent>>> {
        let input = self.input.clone();
        Ok(futures::stream::poll_fn(move |cx| {
            let mut input = input.lock().unwrap();
            if let Some(event) = input.event.take() {
                Poll::Ready(Some(Ok(event)))
            } else {
                // Crossterm retains the initiating waker while its native
                // reader is pending, even when the stream is polled again.
                input.waker.get_or_insert_with(|| cx.waker().clone());
                Poll::Pending
            }
        })
        .boxed())
    }
}

#[derive(Default)]
struct WakeCounter(AtomicUsize);

impl ArcWake for WakeCounter {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn pending_native_input_wakes_and_reaches_an_idle_component() {
    let input = Arc::new(Mutex::new(PendingInput::default()));
    let frames = Arc::new(Mutex::new(Vec::new()));
    let backend = PendingReaderBackend {
        input: input.clone(),
        frames: frames.clone(),
    };
    let mut app = element!(BecomesIdle);
    let mut render_loop = app.render_loop().backend(backend).boxed();
    let counter = Arc::new(WakeCounter::default());
    let waker = futures::task::waker(counter.clone());
    let mut cx = Context::from_waker(&waker);
    assert!(render_loop.as_mut().poll(&mut cx).is_pending());
    assert_eq!(*frames.lock().unwrap(), ["idle\n"]);

    counter.0.store(0, Ordering::SeqCst);
    let pending_reader_waker = {
        let mut input = input.lock().unwrap();
        input.event = Some(TerminalEvent::Key(KeyEvent::new(
            KeyEventKind::Press,
            KeyCode::Char('x'),
        )));
        input.waker.take().expect("the native reader is pending")
    };
    pending_reader_waker.wake();
    assert_eq!(
        counter.0.load(Ordering::SeqCst),
        1,
        "native input must wake the render task before it can be polled again"
    );
    // Drive only scheduled polls, as an executor would. Dispatching the key
    // into the component's event queue can schedule another render-task poll.
    for _ in 0..8 {
        assert!(
            counter.0.swap(0, Ordering::SeqCst) > 0,
            "the render task stalled without another wake"
        );
        if let Poll::Ready(result) = render_loop.as_mut().poll(&mut cx) {
            result.unwrap();
            assert_eq!(*frames.lock().unwrap(), ["idle\n", "key\n"]);
            return;
        }
    }
    panic!("the render task did not deliver the key within eight scheduled polls");
}
