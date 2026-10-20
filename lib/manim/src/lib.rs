//! Real Manim scenes in a note.
//!
//! A ```` ```manim ```` block holds Python for [Manim], the engine
//! 3Blue1Brown's videos are made with: a whole `Scene` class, or just the
//! body of `construct`. [`Manim`] runs the user's own `manim` on it in the
//! background, keeps the last frame — transparent, cropped to what was
//! drawn — and hands it over as a PNG once it is ready. Nothing blocks:
//! asking for a frame that is not there yet starts the render and answers
//! [`State::Running`]; the caller asks again later and watches
//! [`Manim::generation`] to know when.
//!
//! Renders run one at a time on a single worker thread, each with a
//! timeout, and land in a cache directory keyed by the block's text, so
//! a note opened again shows its scenes at once.
//!
//! This runs Python from a note with the user's permissions. The caller
//! decides whether that is allowed; notopod only does it when the user
//! has switched it on.
//!
//! [Manim]: https://www.manim.community

mod crop;
mod script;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use script::script;

/// The fence language tag: ```` ```manim ````.
pub const LANG: &str = "manim";

/// Frames kept in memory before the oldest are dropped. They stay on disk.
const MEMORY: usize = 48;

/// Bumped when the frames' format changes, so old cache files are not
/// mistaken for new ones.
const CACHE_VERSION: u32 = 1;

/// How to run manim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The program and the arguments before notopod's, such as `manim`
    /// or `uv run manim`.
    pub command: Vec<String>,
    /// Render quality: `l`ow (854×480), `m`edium (1280×720) or `h`igh.
    pub quality: char,
    /// How long one scene may take before it is stopped.
    pub timeout: Duration,
    /// Where finished frames are kept between runs.
    pub cache_dir: PathBuf,
}

impl Config {
    /// `manim` at medium quality, a minute per scene, frames in `cache_dir`.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            command: vec!["manim".to_owned()],
            quality: 'm',
            timeout: Duration::from_secs(60),
            cache_dir,
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

struct Shared {
    jobs: Mutex<HashMap<u64, (Job, u64)>>,
    generation: AtomicU64,
    clock: AtomicU64,
}

/// Renders ```` ```manim ```` blocks in the background.
pub struct Manim {
    config: Config,
    shared: Arc<Shared>,
    queue: Mutex<Option<Sender<(u64, String)>>>,
}

impl Manim {
    /// A renderer that runs as `config` says. Nothing is started until a
    /// block is asked for.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            shared: Arc::new(Shared {
                jobs: Mutex::new(HashMap::new()),
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

    /// What there is for `src` now. The first time a block is asked for,
    /// a cached frame is loaded from disk, or a render is queued.
    pub fn state(&self, src: &str) -> State {
        let key = self.key(src);
        let tick = self.shared.clock.fetch_add(1, Ordering::Relaxed);
        {
            let mut jobs = self
                .shared
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((job, used)) = jobs.get_mut(&key) {
                *used = tick;
                return match job {
                    Job::Running => State::Running,
                    Job::Done(Ok(frame)) => State::Ready(Arc::clone(frame)),
                    Job::Done(Err(e)) => State::Failed(e.clone()),
                };
            }
            if let Some(frame) = read_cached(&self.cache_file(key)) {
                let frame = Arc::new(frame);
                insert(&mut jobs, key, Job::Done(Ok(Arc::clone(&frame))), tick);
                return State::Ready(frame);
            }
            insert(&mut jobs, key, Job::Running, tick);
        }
        self.enqueue(key, src);
        State::Running
    }

    /// Like [`Manim::state`], but waits for the render, for printing a
    /// note rather than showing it.
    pub fn wait(&self, src: &str) -> State {
        let deadline = Instant::now() + self.config.timeout + Duration::from_secs(5);
        loop {
            match self.state(src) {
                State::Running if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(40));
                }
                State::Running => return State::Failed("timed out".to_owned()),
                done => return done,
            }
        }
    }

    fn cache_file(&self, key: u64) -> PathBuf {
        self.config.cache_dir.join(format!("{key:016x}.png"))
    }

    fn enqueue(&self, key: u64, src: &str) {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.is_none() {
            let (tx, rx) = mpsc::channel::<(u64, String)>();
            let shared = Arc::clone(&self.shared);
            let config = self.config.clone();
            let spawned = std::thread::Builder::new()
                .name("manim".to_owned())
                .spawn(move || {
                    for (key, src) in rx {
                        let file = config.cache_dir.join(format!("{key:016x}.png"));
                        let result = render(&config, key, &src, &file).map(Arc::new);
                        let mut jobs = shared
                            .jobs
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let tick = shared.clock.fetch_add(1, Ordering::Relaxed);
                        insert(&mut jobs, key, Job::Done(result), tick);
                        drop(jobs);
                        shared.generation.fetch_add(1, Ordering::Release);
                    }
                });
            match spawned {
                Ok(_) => *queue = Some(tx),
                Err(e) => {
                    self.finish(key, Err(format!("cannot start a render thread: {e}")));
                    return;
                }
            }
        }
        if let Some(tx) = queue.as_ref() {
            if tx.send((key, src.to_owned())).is_err() {
                self.finish(key, Err("the render thread has stopped".to_owned()));
            }
        }
    }

    fn finish(&self, key: u64, result: Result<Arc<Frame>, String>) {
        let mut jobs = self
            .shared
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tick = self.shared.clock.fetch_add(1, Ordering::Relaxed);
        insert(&mut jobs, key, Job::Done(result), tick);
        self.shared.generation.fetch_add(1, Ordering::Release);
    }
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

/// Runs manim on one block and keeps its last frame, cropped, in `file`.
fn render(config: &Config, key: u64, src: &str, file: &Path) -> Result<Frame, String> {
    let (code, scene) = script(src);
    let work = config.cache_dir.join("work").join(format!("{key:016x}"));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    let result = run(config, &work, &code, &scene).and_then(|raw| {
        let png = crop::crop(&raw).unwrap_or(raw);
        let (width, height) = png_size(&png).ok_or("manim wrote something that is not a PNG")?;
        // A cache that cannot be written is a slower next start, not an error.
        let _ = fs::write(file, &png);
        Ok(Frame { png, width, height })
    });
    let _ = fs::remove_dir_all(&work);
    // Only succeeds once no other render is using it.
    if let Some(parent) = work.parent() {
        let _ = fs::remove_dir(parent);
    }
    result
}

fn run(config: &Config, work: &Path, code: &str, scene: &str) -> Result<Vec<u8>, String> {
    fs::write(work.join("scene.py"), code).map_err(|e| format!("cannot write the scene: {e}"))?;
    let Some((program, before)) = config.command.split_first() else {
        return Err("no manim command configured".to_owned());
    };
    let log = fs::File::create(work.join("log.txt")).map_err(|e| e.to_string())?;
    let quality = format!("-q{}", config.quality);
    let mut child = Command::new(program)
        .args(before)
        .args([
            "render",
            "--save_last_frame",
            "--transparent",
            "--format=png",
            &quality,
            "--media_dir",
            "media",
            "--output_file",
            "frame",
            "--disable_caching",
            "--progress_bar",
            "none",
            "--verbosity",
            "ERROR",
            "scene.py",
            scene,
        ])
        .current_dir(work)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("{program} not found: install Manim (https://www.manim.community) or set [manim] command")
            } else {
                format!("cannot start {program}: {e}")
            }
        })?;

    let deadline = Instant::now() + config.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(30)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("stopped after {}s", config.timeout.as_secs()));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    if !status.success() {
        return Err(last_error(&work.join("log.txt")));
    }
    let frame = work.join("media/images/scene/frame.png");
    fs::read(&frame).map_err(|_| "manim finished but left no frame".to_owned())
}

/// The useful end of what manim printed: Python's last line, which is
/// the error, rather than the whole traceback.
fn last_error(log: &Path) -> String {
    let mut text = String::new();
    if let Ok(mut f) = fs::File::open(log) {
        let _ = f.read_to_string(&mut text);
    }
    let line = text
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty() && !l.starts_with('│') && !l.starts_with('╭') && !l.starts_with('╰')
        })
        .rev()
        .find(|l| l.contains("Error") || l.contains("error") || l.contains("Exception"))
        .or_else(|| text.lines().map(str::trim).rev().find(|l| !l.is_empty()));
    line.map_or_else(|| "manim failed".to_owned(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_manim(dir: &Path, body: &str) -> Config {
        let program = dir.join("fake-manim");
        fs::write(&program, format!("#!/bin/sh\n{body}\n")).expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        Config {
            command: vec![program.display().to_string()],
            quality: 'l',
            timeout: Duration::from_secs(5),
            cache_dir: dir.join("cache"),
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

    #[test]
    #[cfg(unix)]
    fn renders_in_the_background_caches_and_reloads_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let png = dir.path().join("frame.png");
        fs::write(&png, tiny_png()).expect("write");
        // Stands in for manim: copies a frame to where manim puts one.
        let config = fake_manim(
            dir.path(),
            &format!(
                "mkdir -p media/images/scene && cp {} media/images/scene/frame.png",
                png.display()
            ),
        );
        fs::create_dir_all(&config.cache_dir).expect("mkdir");
        let m = Manim::new(config.clone());
        assert_eq!(m.state("self.add(Circle())"), State::Running);
        let State::Ready(frame) = m.wait("self.add(Circle())") else {
            panic!("not ready");
        };
        assert!(m.generation() >= 1, "finishing bumps the generation");
        assert_eq!(
            (frame.width, frame.height),
            (1, 1),
            "cropped to what was drawn"
        );
        assert!(dir.path().join("cache").read_dir().expect("cache").count() >= 1);

        // A fresh renderer finds it on disk without running anything.
        let again = Manim::new(Config {
            command: vec!["/nonexistent/manim".to_owned()],
            ..config
        });
        assert!(matches!(again.state("self.add(Circle())"), State::Ready(_)));
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
}
