use futures::{channel::mpsc, stream::BoxStream, StreamExt};
use iocraft::{prelude::*, Canvas};
use std::{
    io,
    sync::{Arc, Mutex},
};

#[component]
fn SizedContent(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut redraws = hooks.use_state(|| 0);
    let mut quit = hooks.use_state(|| false);
    hooks.use_terminal_events(move |event| {
        if let TerminalEvent::Key(key) = event {
            if key.code == KeyCode::Esc {
                quit.set(true);
            } else {
                redraws += 1;
            }
        }
    });
    if quit.get() {
        hooks.use_context_mut::<SystemContext>().exit();
    }
    let (width, height) = hooks.use_terminal_size();
    element!(View(width: width, height: height) {
        Text(content: format!("{width}x{height} redraw {}", redraws.get()))
    })
}

struct ResizingBackend {
    frames: Arc<Mutex<Vec<(usize, usize)>>>,
    sender: mpsc::UnboundedSender<io::Result<TerminalEvent>>,
    receiver: Option<mpsc::UnboundedReceiver<io::Result<TerminalEvent>>>,
    stale_resize_event: bool,
}

impl ResizingBackend {
    fn dimensions(&self) -> (u16, u16) {
        let count = self.frames.lock().unwrap().len();
        [(80, 25), (60, 20), (100, 30), (30, 10)][count.min(3)]
    }
}

impl TerminalBackend for ResizingBackend {
    fn size(&self) -> Option<(u16, u16)> {
        Some(self.dimensions())
    }

    fn clear_canvas(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write_canvas(&mut self, _: Option<&Canvas>, canvas: &Canvas) -> io::Result<()> {
        let viewport = self.dimensions();
        if canvas.width() > usize::from(viewport.0) || canvas.height() > usize::from(viewport.1) {
            return Err(io::Error::other(format!(
                "frame {}x{} exceeds viewport {}x{}",
                canvas.width(),
                canvas.height(),
                viewport.0,
                viewport.1
            )));
        }
        assert!(canvas
            .to_string()
            .contains(&format!("{}x{}", viewport.0, viewport.1)));
        let mut frames = self.frames.lock().unwrap();
        frames.push((canvas.width(), canvas.height()));
        let event = if frames.len() == 4 {
            TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Esc))
        } else if self.stale_resize_event {
            // A delayed event from an older viewport must not override size().
            TerminalEvent::Resize(120, 40)
        } else {
            // Ordinary input causes a redraw without delivering any Resize.
            TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('x')))
        };
        self.sender.unbounded_send(Ok(event)).unwrap();
        Ok(())
    }

    fn print_above(&mut self, _: &[Passthrough<'_>]) -> io::Result<()> {
        Ok(())
    }

    fn event_stream(&mut self) -> io::Result<BoxStream<'static, io::Result<TerminalEvent>>> {
        Ok(self.receiver.take().unwrap().boxed())
    }
}

fn check_resizes(stale_resize_event: bool) {
    let frames = Arc::new(Mutex::new(Vec::new()));
    let (sender, receiver) = mpsc::unbounded();
    let backend = ResizingBackend {
        frames: frames.clone(),
        sender,
        receiver: Some(receiver),
        stale_resize_event,
    };
    smol::block_on(element!(SizedContent).render_loop().backend(backend)).unwrap();
    assert_eq!(
        *frames.lock().unwrap(),
        [(80, 25), (60, 20), (100, 30), (30, 10)]
    );
}

#[test]
fn redraw_uses_current_backend_size_before_resize_event_arrives() {
    check_resizes(false);
}

#[test]
fn delayed_resize_event_does_not_override_current_backend_size() {
    check_resizes(true);
}
