//! Real Manim scenes in a note.
//!
//! A ```` ```manim ```` block holds Python for [Manim], the engine
//! 3Blue1Brown's videos are made with: a whole `Scene` class, or just the
//! body of `construct`. [`Manim`] renders it with the user's own manim in
//! the background, keeps the last frame — transparent, cropped to what
//! was drawn — and hands it over as a PNG once it is ready. Nothing
//! blocks: asking for a frame that is not there yet starts the render and
//! answers [`State::Running`]; the caller asks again later and watches
//! [`Manim::generation`] to know when.
//!
//! Renders run one at a time on one thread, through a Python worker that
//! has manim imported already (see `engine`), so a scene takes about as
//! long as drawing it: tens of milliseconds for a still. The worker
//! starts with the first render and stops after a while without one.
//! Frames are cached on disk by the block's text, so a note opened again
//! shows its scenes without starting anything.
//!
//! [`Manim::live`] is for the block being typed in: it waits for a pause
//! in the typing, and a render the typing has overtaken is stopped.
//!
//! This runs Python from a note with the user's permissions.
//!
//! [Manim]: https://www.manim.community

mod crop;
mod engine;
mod native;
mod script;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use engine::{Engine, Outcome};
pub use script::script;

/// The fence language tag: ```` ```manim ````.
pub const LANG: &str = "manim";

/// Frames kept in memory before the oldest are dropped. They stay on disk.
const MEMORY: usize = 48;

/// Bumped when the frames' format changes, so old cache files are not
/// mistaken for new ones.
const CACHE_VERSION: u32 = 1;

/// How long typing has to pause before the block being typed is rendered.
const LIVE_PAUSE: Duration = Duration::from_millis(300);

/// How long the worker is kept with nothing to render. It holds manim in
/// memory, a hundred megabytes or so.
const IDLE: Duration = Duration::from_secs(120);

/// How to run manim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The program and the arguments before notopod's, such as `manim`
    /// or `uv run manim`.
    pub command: Vec<String>,
    /// The Python that has manim installed. Found from `command` when it
    /// is not given; with neither, every render runs `command` afresh,
    /// which is slower.
    pub python: Option<PathBuf>,
    /// Render quality: `l`ow (854×480), `m`edium (1280×720) or `h`igh.
    pub quality: char,
    /// How long one scene may take before it is stopped.
    pub timeout: Duration,
    /// Where finished frames are kept between runs.
    pub cache_dir: PathBuf,
    /// Draw the scenes notopod understands itself, and only send the
    /// rest to manim.
    pub native: bool,
}

impl Config {
    /// `manim` at medium quality, a minute per scene, frames in
    /// `cache_dir`, simple scenes drawn natively.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            command: vec!["manim".to_owned()],
            python: None,
            quality: 'm',
            timeout: Duration::from_secs(60),
            cache_dir,
            native: true,
        }
    }
}

/// A finished frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The picture, a PNG with a transparent background.
    pub png: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Where a block's render is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Queued or being rendered.
    Running,
    /// Done.
    Ready(Arc<Frame>),
    /// manim failed, timed out, or could not be started; what it said.
    Failed(String),
}

#[derive(Debug, Clone)]
enum Job {
    Running,
    Done(Result<Arc<Frame>, String>),
}

/// The block being typed in, waiting for a pause.
struct Live {
    key: u64,
    src: String,
    since: Instant,
}

struct Shared {
    jobs: Mutex<HashMap<u64, (Job, u64)>>,
    live: Mutex<Option<Live>>,
    generation: AtomicU64,
    clock: AtomicU64,
}

impl Shared {
    fn jobs(&self) -> MutexGuard<'_, HashMap<u64, (Job, u64)>> {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn live(&self) -> MutexGuard<'_, Option<Live>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn done(&self, key: u64) -> bool {
        matches!(self.jobs().get(&key), Some((Job::Done(_), _)))
    }

    fn finish(&self, key: u64, result: Result<Arc<Frame>, String>) {
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);
        insert(&mut self.jobs(), key, Job::Done(result), tick);
        self.generation.fetch_add(1, Ordering::Release);
    }
}

enum Message {
    Render(u64, String),
    /// Something changed in `Shared::live`.
    Wake,
}

/// Renders ```` ```manim ```` blocks in the background.
pub struct Manim {
    config: Config,
    shared: Arc<Shared>,
    queue: Mutex<Option<Sender<Message>>>,
}

impl Manim {
    /// A renderer that runs as `config` says. Nothing is started until a
    /// block is asked for.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            shared: Arc::new(Shared {
                jobs: Mutex::new(HashMap::new()),
                live: Mutex::new(None),
                generation: AtomicU64::new(0),
                clock: AtomicU64::new(0),
            }),
            queue: Mutex::new(None),
        }
    }

    /// The cache key of a block: its text and the settings that change
    /// the picture.
    pub fn key(&self, src: &str) -> u64 {
        let mut h = DefaultHasher::new();
        CACHE_VERSION.hash(&mut h);
        src.hash(&mut h);
        self.config.quality.hash(&mut h);
        h.finish()
    }

    /// Goes up by one every time a render finishes. A screen that shows
    /// frames redraws when it changes.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }

    /// Whether a render is queued, running or waiting for a pause: a
    /// screen showing frames should look again soon.
    pub fn busy(&self) -> bool {
        let live = self
            .shared
            .live()
            .as_ref()
            .is_some_and(|l| !self.shared.done(l.key));
        live || self
            .shared
            .jobs()
            .values()
            .any(|(job, _)| matches!(job, Job::Running))
    }

    /// What there is for `src` now. The first time a block is asked for,
    /// a cached frame is loaded from disk, or a render is queued.
    pub fn state(&self, src: &str) -> State {
        let key = self.key(src);
        if let Some(state) = self.known(key).or_else(|| self.native(key, src)) {
            return state;
        }
        let tick = self.shared.clock.fetch_add(1, Ordering::Relaxed);
        insert(&mut self.shared.jobs(), key, Job::Running, tick);
        self.send(Message::Render(key, src.to_owned()));
        State::Running
    }

    /// Like [`Manim::state`], for the block the cursor is in, whose text
    /// changes with every key. It is rendered once the typing pauses; a
    /// render of text typed over since is stopped, so the latest text is
    /// always the next one drawn.
    pub fn live(&self, src: &str) -> State {
        let key = self.key(src);
        if let Some(state) = self.known(key) {
            return state;
        }
        if let Some(state) = self.native(key, src) {
            // Drawn already: whatever manim was doing for older text can stop.
            *self.shared.live() = None;
            return state;
        }
        let mut live = self.shared.live();
        if live.as_ref().is_none_or(|l| l.key != key) {
            *live = Some(Live {
                key,
                src: src.to_owned(),
                since: Instant::now(),
            });
            drop(live);
            self.send(Message::Wake);
        }
        State::Running
    }

    /// Like [`Manim::state`], but waits for the render, for printing a
    /// note rather than showing it.
    pub fn wait(&self, src: &str) -> State {
        let deadline = Instant::now() + self.config.timeout + Duration::from_secs(5);
        loop {
            match self.state(src) {
                State::Running if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                State::Running => return State::Failed("timed out".to_owned()),
                done => return done,
            }
        }
    }

    /// The block drawn here, without manim, when notopod understands all
    /// of it; `None` sends it to manim.
    fn native(&self, key: u64, src: &str) -> Option<State> {
        if !self.config.native {
            return None;
        }
        let result = match native::render(src, self.config.quality) {
            Ok(png) => png_size(&png)
                .map(|(width, height)| Arc::new(Frame { png, width, height }))
                .ok_or_else(|| "could not draw this scene".to_owned()),
            Err(native::Error::Unsupported(_)) => return None,
            Err(native::Error::Syntax(e)) => Err(format!("SyntaxError: {e}")),
            Err(native::Error::Empty) => {
                Err("nothing on screen: show something with self.add or self.play".to_owned())
            }
        };
        let state = match &result {
            Ok(frame) => State::Ready(Arc::clone(frame)),
            Err(e) => State::Failed(e.clone()),
        };
        let tick = self.shared.clock.fetch_add(1, Ordering::Relaxed);
        insert(&mut self.shared.jobs(), key, Job::Done(result), tick);
        Some(state)
    }

    /// What is known about `key` without rendering: a finished job, a
    /// render on its way, or a frame on disk.
    fn known(&self, key: u64) -> Option<State> {
        let tick = self.shared.clock.fetch_add(1, Ordering::Relaxed);
        let mut jobs = self.shared.jobs();
        if let Some((job, used)) = jobs.get_mut(&key) {
            *used = tick;
            return Some(match job {
                Job::Running => State::Running,
                Job::Done(Ok(frame)) => State::Ready(Arc::clone(frame)),
                Job::Done(Err(e)) => State::Failed(e.clone()),
            });
        }
        let frame = Arc::new(read_cached(&cache_file(&self.config, key))?);
        insert(&mut jobs, key, Job::Done(Ok(Arc::clone(&frame))), tick);
        Some(State::Ready(frame))
    }

    fn send(&self, message: Message) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.is_none() {
            let (tx, rx) = mpsc::channel();
            let shared = Arc::clone(&self.shared);
            let config = self.config.clone();
            let spawned = std::thread::Builder::new()
                .name("manim".to_owned())
                .spawn(move || work(&rx, &shared, &config));
            if let Err(e) = spawned {
                drop(queue);
                if let Message::Render(key, _) = message {
                    self.shared
                        .finish(key, Err(format!("cannot start a render thread: {e}")));
                }
                return;
            }
            *queue = Some(tx);
        }
        if let Some(tx) = queue.as_ref() {
            if let Err(mpsc::SendError(Message::Render(key, _))) = tx.send(message) {
                self.shared
                    .finish(key, Err("the render thread has stopped".to_owned()));
            }
        }
    }
}

/// The render thread: the live block once typing pauses, everything else
/// in the order asked for, one at a time.
fn work(rx: &Receiver<Message>, shared: &Shared, config: &Config) {
    let mut queue: VecDeque<(u64, String)> = VecDeque::new();
    let mut engine: Option<Engine> = None;
    let mut last_used = Instant::now();
    loop {
        let live = shared
            .live()
            .as_ref()
            .filter(|l| !shared.done(l.key))
            .map(|l| (l.key, l.src.clone(), l.since.elapsed()));
        let next = match &live {
            Some((key, src, waited)) if *waited >= LIVE_PAUSE => Some((*key, src.clone(), true)),
            _ => queue.pop_front().map(|(key, src)| (key, src, false)),
        };

        if let Some((key, src, is_live)) = next {
            if shared.done(key) {
                continue;
            }
            let running = engine.get_or_insert_with(|| Engine::start(config));
            let wanted = || !is_live || shared.live().as_ref().is_some_and(|l| l.key == key);
            match render(running, config, key, &src, &wanted) {
                Outcome::Frame(png) => shared.finish(key, frame(config, key, png)),
                Outcome::Failed(why) => shared.finish(key, Err(without_manim(config, &src, why))),
                // A newer live block is waiting; it is taken next.
                Outcome::Cancelled => {}
            }
            if !running.alive() {
                engine = None;
            }
            last_used = Instant::now();
            continue;
        }

        // Nothing to do yet. The block being typed will want the worker in
        // a moment, so it is started now rather than after the pause.
        let wait = if let Some((_, _, waited)) = &live {
            if engine.is_none() {
                engine = Some(Engine::start(config));
                last_used = Instant::now();
                continue;
            }
            LIVE_PAUSE.saturating_sub(*waited)
        } else if engine.is_some() {
            IDLE.saturating_sub(last_used.elapsed())
        } else {
            Duration::from_secs(3600)
        };
        match rx.recv_timeout(wait) {
            Ok(Message::Render(key, src)) => queue.push_back((key, src)),
            Ok(Message::Wake) => {}
            Err(RecvTimeoutError::Timeout) => {
                if live.is_none() && last_used.elapsed() >= IDLE {
                    engine = None;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// When manim is not installed, what in the block needed it.
fn without_manim(config: &Config, src: &str, why: String) -> String {
    if !config.native || !why.contains("not found") {
        return why;
    }
    match native::render(src, config.quality) {
        Err(native::Error::Unsupported(what)) => format!(
            "notopod draws simple scenes itself, but not {what}; that needs Manim (https://www.manim.community)"
        ),
        _ => why,
    }
}

/// Renders one block in a scratch directory of its own.
fn render(
    engine: &mut Engine,
    config: &Config,
    key: u64,
    src: &str,
    wanted: &dyn Fn() -> bool,
) -> Outcome {
    let (code, scene) = script(src);
    let work = config.cache_dir.join("work").join(format!("{key:016x}"));
    let _ = fs::remove_dir_all(&work);
    if let Err(e) = fs::create_dir_all(&work) {
        return Outcome::Failed(format!("cannot create {}: {e}", work.display()));
    }
    let outcome = match fs::write(work.join("scene.py"), code) {
        Ok(()) => engine.render(config, &work, &scene, wanted),
        Err(e) => Outcome::Failed(format!("cannot write the scene: {e}")),
    };
    let _ = fs::remove_dir_all(&work);
    // Only succeeds once no other render is using it.
    if let Some(parent) = work.parent() {
        let _ = fs::remove_dir(parent);
    }
    outcome
}

/// A raw frame cropped, kept on disk and measured.
fn frame(config: &Config, key: u64, raw: Vec<u8>) -> Result<Arc<Frame>, String> {
    let png = crop::crop(&raw).unwrap_or(raw);
    let (width, height) = png_size(&png).ok_or("manim wrote something that is not a PNG")?;
    // A cache that cannot be written is a slower next start, not an error.
    let _ = fs::write(cache_file(config, key), &png);
    Ok(Arc::new(Frame { png, width, height }))
}

fn cache_file(config: &Config, key: u64) -> PathBuf {
    config.cache_dir.join(format!("{key:016x}.png"))
}

/// Records a job, dropping the least recently asked-for finished ones
/// once there are too many. Running jobs are never dropped.
fn insert(jobs: &mut HashMap<u64, (Job, u64)>, key: u64, job: Job, tick: u64) {
    jobs.insert(key, (job, tick));
    while jobs.len() > MEMORY {
        let Some(oldest) = jobs
            .iter()
            .filter(|(_, (job, _))| matches!(job, Job::Done(_)))
            .min_by_key(|(_, (_, used))| *used)
            .map(|(k, _)| *k)
        else {
            break;
        };
        jobs.remove(&oldest);
    }
}

fn read_cached(path: &Path) -> Option<Frame> {
    let png = fs::read(path).ok()?;
    let (width, height) = png_size(&png)?;
    Some(Frame { png, width, height })
}

/// Width and height from a PNG's header.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn executable(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("write");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    /// A `manim` command that runs `body` in the scene's directory.
    #[cfg(unix)]
    fn fake_manim(dir: &Path, body: &str) -> Config {
        let program = dir.join("fake-manim");
        executable(&program, body);
        Config {
            command: vec![program.display().to_string()],
            python: None,
            quality: 'l',
            timeout: Duration::from_secs(5),
            cache_dir: dir.join("cache"),
            native: false,
        }
    }

    /// A Python that speaks the worker's protocol: every render copies
    /// `frame` into place, after `delay` seconds. It notes each start in
    /// `starts` and each scene directory in `renders`.
    #[cfg(unix)]
    fn fake_worker(dir: &Path, frame: &Path, delay: &str) -> Config {
        let python = dir.join("fake-python");
        executable(
            &python,
            &format!(
                r#"echo start >> {starts}
echo "ready	test"
tab=$(printf '\t')
while IFS="$tab" read -r cmd id quality scene work secs; do
  if [ "$cmd" = render ]; then
    sleep {delay}
    echo "$work" >> {renders}
    mkdir -p "$work/media/images" && cp {frame} "$work/media/images/frame.png"
    echo "done	$id"
  fi
done"#,
                starts = dir.join("starts").display(),
                renders = dir.join("renders").display(),
                frame = frame.display(),
            ),
        );
        Config {
            command: vec!["/nonexistent/manim".to_owned()],
            python: Some(python),
            quality: 'l',
            timeout: Duration::from_secs(5),
            cache_dir: dir.join("cache"),
            native: false,
        }
    }

    /// A 4×2 RGBA PNG with one opaque pixel.
    fn tiny_png() -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 4, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut data = vec![0u8; 4 * 2 * 4];
        data[4 * 5 + 3] = 255;
        enc.write_header()
            .and_then(|mut w| w.write_image_data(&data))
            .expect("png");
        out
    }

    #[cfg(unix)]
    fn lines(path: &Path) -> usize {
        fs::read_to_string(path).map_or(0, |s| s.lines().count())
    }

    #[test]
    #[cfg(unix)]
    fn one_worker_renders_everything_and_the_disk_remembers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let png = dir.path().join("frame.png");
        fs::write(&png, tiny_png()).expect("write");
        let config = fake_worker(dir.path(), &png, "0");
        let m = Manim::new(config.clone());
        assert_eq!(m.state("self.add(Circle())"), State::Running);
        let State::Ready(frame) = m.wait("self.add(Circle())") else {
            panic!("not ready");
        };
        assert_eq!(
            (frame.width, frame.height),
            (1, 1),
            "cropped to what was drawn"
        );
        assert!(m.generation() >= 1, "finishing bumps the generation");
        assert!(matches!(m.wait("self.add(Square())"), State::Ready(_)));
        assert_eq!(lines(&dir.path().join("starts")), 1, "the worker is reused");
        assert_eq!(lines(&dir.path().join("renders")), 2);

        // A fresh renderer finds both on disk without starting anything.
        let again = Manim::new(Config {
            python: Some(PathBuf::from("/nonexistent/python")),
            ..config
        });
        assert!(matches!(again.state("self.add(Circle())"), State::Ready(_)));
        assert!(!again.busy());
    }

    #[test]
    #[cfg(unix)]
    fn the_live_block_waits_for_a_pause_and_only_the_latest_is_drawn() {
        let dir = tempfile::tempdir().expect("tempdir");
        let png = dir.path().join("frame.png");
        fs::write(&png, tiny_png()).expect("write");
        let m = Manim::new(fake_worker(dir.path(), &png, "0"));
        // Typing: every key is new text, well inside the pause.
        for text in ["self.add(C", "self.add(Ci", "self.add(Circle())"] {
            assert_eq!(m.live(text), State::Running);
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(m.busy());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(m.live("self.add(Circle())"), State::Ready(_)) {
            assert!(Instant::now() < deadline, "the live block was never drawn");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            lines(&dir.path().join("renders")),
            1,
            "only the text typing stopped at"
        );
        assert!(!m.busy());
    }

    #[test]
    #[cfg(unix)]
    fn typing_on_stops_a_live_render_that_is_out_of_date() {
        let dir = tempfile::tempdir().expect("tempdir");
        let png = dir.path().join("frame.png");
        fs::write(&png, tiny_png()).expect("write");
        // A render takes long enough to be overtaken.
        let m = Manim::new(fake_worker(dir.path(), &png, "1"));
        m.live("first");
        std::thread::sleep(LIVE_PAUSE + Duration::from_millis(150));
        m.live("second");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(m.live("second"), State::Ready(_)) {
            assert!(Instant::now() < deadline, "second was never drawn");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !matches!(m.state_if_known("first"), Some(State::Ready(_))),
            "first was given up, not finished"
        );
    }

    #[test]
    #[cfg(unix)]
    fn without_a_python_the_command_is_run() {
        let dir = tempfile::tempdir().expect("tempdir");
        let png = dir.path().join("frame.png");
        fs::write(&png, tiny_png()).expect("write");
        // A `#!/bin/sh` script names no Python, so the command is used.
        let config = fake_manim(
            dir.path(),
            &format!(
                "mkdir -p media/images/scene && cp {} media/images/scene/frame.png",
                png.display()
            ),
        );
        let m = Manim::new(config);
        assert!(matches!(m.wait("self.add(Circle())"), State::Ready(_)));
    }

    #[test]
    #[cfg(unix)]
    fn failures_say_what_went_wrong() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = fake_manim(
            dir.path(),
            "echo 'Traceback (most recent call last):' >&2; echo 'NameError: name foo is not defined' >&2; exit 1",
        );
        let m = Manim::new(config.clone());
        assert_eq!(
            m.wait("foo"),
            State::Failed("NameError: name foo is not defined".to_owned())
        );

        let missing = Manim::new(Config {
            command: vec!["/nonexistent/manim".to_owned()],
            ..config.clone()
        });
        assert!(matches!(missing.wait("x"), State::Failed(e) if e.contains("not found")));

        let slow = Manim::new(Config {
            timeout: Duration::from_millis(200),
            ..fake_manim(dir.path(), "sleep 5")
        });
        assert!(matches!(slow.wait("y"), State::Failed(e) if e.contains("stopped")));
    }

    #[test]
    fn simple_scenes_are_drawn_without_manim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let m = Manim::new(Config {
            command: vec!["/nonexistent/manim".to_owned()],
            python: Some(PathBuf::from("/nonexistent/python")),
            ..Config::new(dir.path().join("cache"))
        });
        assert!(
            matches!(m.state("self.add(Circle())"), State::Ready(_)),
            "at once"
        );
        assert!(matches!(m.live("self.add(Square())"), State::Ready(_)));
        assert!(!m.busy(), "nothing was started");
        assert!(
            matches!(m.live("self.add(Square("), State::Failed(e) if e.starts_with("SyntaxError"))
        );
        // What notopod cannot draw goes to manim, which is missing here,
        // and the message says what needed it.
        let State::Failed(why) = m.wait("for i in range(3):\n    self.add(Dot())") else {
            panic!("not failed");
        };
        assert!(
            why.contains("`for`") && why.contains("needs Manim"),
            "{why}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn what_notopod_cannot_draw_goes_to_manim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let png = dir.path().join("frame.png");
        fs::write(&png, tiny_png()).expect("write");
        let m = Manim::new(Config {
            native: true,
            ..fake_worker(dir.path(), &png, "0")
        });
        assert!(matches!(m.state("self.add(Circle())"), State::Ready(_)));
        assert_eq!(lines(&dir.path().join("starts")), 0, "drawn natively");
        assert!(matches!(m.wait("self.add(ThreeDAxes())"), State::Ready(_)));
        assert_eq!(lines(&dir.path().join("renders")), 1, "by the worker");
    }

    #[test]
    fn keys_follow_the_text_and_the_quality() {
        let m = Manim::new(Config::new(PathBuf::from("/tmp/x")));
        assert_eq!(m.key("a"), m.key("a"));
        assert_ne!(m.key("a"), m.key("b"));
        let high = Manim::new(Config {
            quality: 'h',
            ..Config::new(PathBuf::from("/tmp/x"))
        });
        assert_ne!(m.key("a"), high.key("a"));
        assert_eq!(png_size(&tiny_png()), Some((4, 2)));
        assert_eq!(png_size(b"nope"), None);
    }

    #[cfg(unix)]
    impl Manim {
        fn state_if_known(&self, src: &str) -> Option<State> {
            self.known(self.key(src))
        }
    }
}
