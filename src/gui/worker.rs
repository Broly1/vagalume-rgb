//! The thread that owns the transport, so a slow or retrying bus never stalls
//! the window.

use std::path::Path;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use crate::config::Config;
use crate::controller::{restore_frames, send_all};
use crate::device::{open_transport, DryRunTransport, Transport};
use crate::error::Result;
use crate::protocol::Frame;

enum Request {
    Connect(Box<Config>),
    Send(Vec<Frame>),
}

#[derive(Debug)]
pub enum Event {
    /// Connected; `None` means a dry run.
    Connected(Option<u32>),
    Failed(String),
}

pub struct Worker {
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn(dry_run: bool, allow_unknown: bool, events: async_channel::Sender<Event>) -> Self {
        let (requests, inbox) = mpsc::channel();
        let thread = thread::spawn(move || serve(dry_run, allow_unknown, &inbox, &events));
        Self {
            requests: Some(requests),
            thread: Some(thread),
        }
    }

    fn request(&self, request: Request) {
        if let Some(requests) = &self.requests {
            let _ = requests.send(request);
        }
    }

    pub fn connect(&self, config: Config) {
        self.request(Request::Connect(Box::new(config)));
    }

    pub fn send(&self, frames: Vec<Frame>) {
        self.request(Request::Send(frames));
    }

    /// Lets queued frames finish, then closes the bus.
    pub fn finish(&mut self) {
        self.requests = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn open(dry_run: bool, allow_unknown: bool, config: &Config) -> Result<Box<dyn Transport>> {
    let mut transport: Box<dyn Transport> = if dry_run {
        Box::new(DryRunTransport::new(true))
    } else {
        open_transport(allow_unknown, Path::new("/sys"))?
    };
    send_all(transport.as_mut(), &restore_frames(config)?)?;
    Ok(transport)
}

fn serve(
    dry_run: bool,
    allow_unknown: bool,
    inbox: &mpsc::Receiver<Request>,
    events: &async_channel::Sender<Event>,
) {
    let mut transport: Option<Box<dyn Transport>> = None;
    let mut failed = false;
    let report = |event| {
        let _ = events.send_blocking(event);
    };
    while let Ok(request) = inbox.recv() {
        match request {
            Request::Connect(config) => match open(dry_run, allow_unknown, &config) {
                Ok(opened) => {
                    failed = false;
                    report(Event::Connected(opened.bus()));
                    transport = Some(opened);
                }
                Err(error) => {
                    failed = true;
                    report(Event::Failed(error.to_string()));
                }
            },
            Request::Send(frames) => {
                // Frames changed while disconnected go out with the next connect.
                let Some(active) = transport.as_mut() else {
                    continue;
                };
                match send_all(active.as_mut(), &frames) {
                    Ok(()) if failed => {
                        failed = false;
                        report(Event::Connected(active.bus()));
                    }
                    Ok(()) => {}
                    Err(error) => {
                        failed = true;
                        report(Event::Failed(error.to_string()));
                    }
                }
            }
        }
    }
}
