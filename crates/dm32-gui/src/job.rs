//! Radio operations run on a background thread so the UI stays responsive.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use dm32_codeplug::Image;
use dm32_codeplug::radio::{self, Ctx, Progress, ReadOptions, WriteOptions, WriteReport};
use dm32_proto::{
    LineConfig, RadioInfo, SerialTransport, Session, Timing, TracingTransport, Transport,
};

#[derive(Debug, Clone)]
pub struct ConnSettings {
    pub port: String,
    pub lines: LineConfig,
    pub trace: Option<PathBuf>,
}

pub enum Request {
    Probe,
    Read {
        contacts: bool,
    },
    Write {
        image: Box<Image>,
        opts: WriteOptions,
        backup: PathBuf,
    },
}

pub enum Outcome {
    Probed(Box<RadioInfo>),
    Read(Box<Image>),
    Wrote {
        report: WriteReport,
        backup: PathBuf,
    },
    Failed(String),
    Cancelled,
}

enum Msg {
    Progress(Progress),
    Done(Outcome),
}

/// A running radio operation.
pub struct Job {
    pub title: String,
    pub stage: String,
    pub fraction: Option<f32>,
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub fn start(
        title: &str,
        conn: ConnSettings,
        req: Request,
        repaint: impl Fn() + Send + 'static,
    ) -> Job {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        std::thread::spawn(move || {
            let outcome = run(conn, req, &tx, &c, &repaint);
            let _ = tx.send(Msg::Done(outcome));
            repaint();
        });
        Job {
            title: title.into(),
            stage: "Connecting".into(),
            fraction: None,
            rx,
            cancel,
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelling(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Process pending messages; returns the outcome once the job has finished.
    pub fn poll(&mut self) -> Option<Outcome> {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Progress(Progress::Stage(s)) => {
                    self.stage = s;
                    self.fraction = None;
                }
                Msg::Progress(Progress::Step { done, total }) => {
                    self.fraction = Some(done as f32 / total.max(1) as f32);
                }
                Msg::Done(o) => return Some(o),
            }
        }
        None
    }
}

fn run(
    conn: ConnSettings,
    req: Request,
    tx: &Sender<Msg>,
    cancel: &AtomicBool,
    repaint: &dyn Fn(),
) -> Outcome {
    let serial = match SerialTransport::open(&conn.port, conn.lines) {
        Ok(s) => s,
        Err(e) => {
            let mut msg = e.to_string();
            if msg.contains("ermission denied") {
                msg.push_str(
                    "\n\nAdd your user to the 'dialout' group (sudo usermod -aG dialout $USER), then log out and in again.",
                );
            }
            return Outcome::Failed(msg);
        }
    };
    let t: Box<dyn Transport> = match conn.trace.as_ref().map(std::fs::File::create) {
        Some(Ok(f)) => Box::new(TracingTransport::new(serial, Box::new(f))),
        Some(Err(e)) => return Outcome::Failed(format!("cannot create trace file: {e}")),
        None => Box::new(serial),
    };
    let mut s = Session::new(t, Timing::hardware());
    let mut progress = |p: Progress| {
        let _ = tx.send(Msg::Progress(p));
        repaint();
    };
    let mut cx = Ctx {
        progress: &mut progress,
        cancel,
    };

    let result = match req {
        Request::Probe => s.identify().map(|i| Outcome::Probed(Box::new(i.clone()))),
        Request::Read { contacts } => radio::read(
            &mut s,
            ReadOptions {
                contacts,
                oem_compat: true,
            },
            &mut cx,
        )
        .map(|i| Outcome::Read(Box::new(i))),
        Request::Write {
            image,
            opts,
            backup,
        } => (|| {
            let prepared = radio::prepare(&mut s, &image, opts, &mut cx)?;
            if let Some(dir) = backup.parent() {
                std::fs::create_dir_all(dir)?;
            }
            prepared.before.save(&backup).map_err(|e| {
                dm32_proto::Error::Port(format!("cannot save backup, nothing written: {e}"))
            })?;
            let report = radio::execute(&mut s, &prepared, opts, &mut cx)?;
            Ok(Outcome::Wrote { report, backup })
        })(),
    };
    // Dropping the session closes the port, which takes the radio out of programming mode.
    drop(s);
    match result {
        Ok(o) => o,
        Err(dm32_proto::Error::Cancelled) => Outcome::Cancelled,
        Err(e) => Outcome::Failed(format!(
            "{e}\n\nIf the radio was already used since it was switched on, power-cycle it and retry. \
             Some firmware versions need RTS high / DTR low (Radio → Connection settings)."
        )),
    }
}
