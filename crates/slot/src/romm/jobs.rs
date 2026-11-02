use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot_store::{Cart, Platform};

use super::api::{self, Game, Pairing, Poll};
use super::http::Http;
use super::install::{install, Installed, Order, Step};
use super::name::fold;
use super::tls::Pin;
use super::{Login, RommFail, Server};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Library {
    pub shelves: Vec<(Platform, Vec<Game>)>,
}

impl Library {
    pub fn games(&self, platform: Platform) -> &[Game] {
        self.shelves
            .iter()
            .find(|(p, _)| *p == platform)
            .map_or(&[], |(_, g)| g.as_slice())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Library(Server),
    Certificate(Server),
    Exchange(Server, String),
    Pair(Server, String),
    Install {
        server: Server,
        root: PathBuf,
        platform: Platform,
        game: Game,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Library(Result<Library, RommFail>),
    Certificate(Result<Pin, RommFail>),
    Token(Result<Login, RommFail>),
    Pairing(Pairing),
    Step(Step),
    Installed(Result<Installed, RommFail>),
}

pub trait RommJobs: Send {
    fn ask(&mut self, job: Job);
    fn take(&mut self) -> Option<Event>;
    fn cancel(&mut self);
}

pub type FallbackLabel = Arc<dyn Fn(&Path, &Cart) -> bool + Send + Sync>;

struct Worker {
    jobs: Sender<(Job, Arc<AtomicBool>)>,
    events: Receiver<Event>,
    stop: Arc<AtomicBool>,
}

impl RommJobs for Worker {
    fn ask(&mut self, job: Job) {
        self.stop = Arc::new(AtomicBool::new(false));
        let _ = self.jobs.send((job, self.stop.clone()));
    }

    fn take(&mut self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    fn cancel(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn romm_jobs(http: Arc<dyn Http>, fallback: FallbackLabel) -> Box<dyn RommJobs> {
    let (jobs, rx) = channel::<(Job, Arc<AtomicBool>)>();
    let (tx, events) = channel();
    let spawned = std::thread::Builder::new()
        .name("slot-romm".into())
        .spawn(move || {
            for (job, stop) in rx {
                let cancelled = || stop.load(Ordering::SeqCst);
                run(http.as_ref(), &fallback, job, &cancelled, &mut |e| {
                    let _ = tx.send(e);
                });
            }
        });
    if let Err(e) = spawned {
        eprintln!("slot: romm: worker thread failed to start: {e}");
    }
    Box::new(Worker {
        jobs,
        events,
        stop: Arc::new(AtomicBool::new(false)),
    })
}

pub fn device_romm_jobs() -> Box<dyn RommJobs> {
    let fallback: FallbackLabel =
        Arc::new(|root, cart| crate::art::jobs::fetch_label(root, cart).unwrap_or(false));
    romm_jobs(Arc::new(super::http::Ureq), fallback)
}

pub fn run(
    http: &dyn Http,
    fallback: &FallbackLabel,
    job: Job,
    cancelled: &dyn Fn() -> bool,
    send: &mut dyn FnMut(Event),
) {
    if crate::update::wait_online().is_err() {
        return send(match job {
            Job::Library(_) => Event::Library(Err(RommFail::Offline)),
            Job::Certificate(_) => Event::Certificate(Err(RommFail::Offline)),
            Job::Exchange(..) | Job::Pair(..) => Event::Token(Err(RommFail::Offline)),
            Job::Install { .. } => Event::Installed(Err(RommFail::Offline)),
        });
    }
    match job {
        Job::Library(server) => send(Event::Library(library(http, &server, cancelled))),
        Job::Certificate(server) => send(Event::Certificate(http.certificate(&server.url("/")))),
        Job::Exchange(server, code) => send(Event::Token(
            api::exchange_code(http, &server, &code).map(Login::Token),
        )),
        Job::Pair(server, device) => pair(http, &server, &device, cancelled, send),
        Job::Install {
            server,
            root,
            platform,
            game,
        } => {
            let order = Order {
                server: &server,
                root: &root,
                platform,
                game: &game,
            };
            let result = install(
                http,
                order,
                cancelled,
                &mut |s| send(Event::Step(s)),
                &|cart| fallback(&root, cart),
            );
            send(Event::Installed(result));
        }
    }
}

pub fn library(
    http: &dyn Http,
    server: &Server,
    cancelled: &dyn Fn() -> bool,
) -> Result<Library, RommFail> {
    let mut out = Library::default();
    for platform in [Platform::Gba, Platform::Gbc, Platform::Gb] {
        out.shelves.push((platform, Vec::new()));
    }
    for shelf in api::shelves(http, server)? {
        let games = api::games(http, server, &shelf, cancelled)?;
        if let Some((_, list)) = out.shelves.iter_mut().find(|(p, _)| *p == shelf.platform) {
            list.extend(games.into_iter().filter(|g| !g.missing));
        }
    }
    for (_, list) in &mut out.shelves {
        list.sort_by_cached_key(|g| (fold(g.title()), g.id));
        list.dedup_by_key(|g| g.id);
    }
    Ok(out)
}

fn pair(
    http: &dyn Http,
    server: &Server,
    device: &str,
    cancelled: &dyn Fn() -> bool,
    send: &mut dyn FnMut(Event),
) {
    let pairing = match api::start_pairing(http, server, device) {
        Ok(p) => p,
        Err(e) => return send(Event::Token(Err(e))),
    };
    send(Event::Pairing(pairing.clone()));
    let ends = Instant::now() + pairing.expires_in;
    let mut wait = pairing.interval;
    loop {
        let wake = Instant::now() + wait;
        while Instant::now() < wake {
            if cancelled() {
                return send(Event::Token(Err(RommFail::Cancelled)));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if Instant::now() > ends {
            return send(Event::Token(Err(RommFail::Expired)));
        }
        match api::poll_pairing(http, server, &pairing) {
            Ok(Poll::Token(t)) => return send(Event::Token(Ok(Login::Token(t)))),
            Ok(Poll::Waiting) => {}
            Ok(Poll::SlowDown) => wait += Duration::from_secs(5),
            Err(e) => return send(Event::Token(Err(e))),
        }
    }
}

struct NoJobs;

impl RommJobs for NoJobs {
    fn ask(&mut self, _: Job) {}

    fn take(&mut self) -> Option<Event> {
        None
    }

    fn cancel(&mut self) {}
}

pub fn no_romm_jobs() -> Box<dyn RommJobs> {
    Box::new(NoJobs)
}
