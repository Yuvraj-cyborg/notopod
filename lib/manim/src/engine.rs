//! Running manim: a warm Python worker where there is one, the `manim`
//! command where there is not.
//!
//! The `manim render` command is slow to start for reasons that have
//! nothing to do with the scene: it imports manim afresh every time, and
//! asks PyPI whether it is out of date first, which can hang for as long
//! as the network takes to give up. The worker imports manim once and
//! forks for each scene (`worker.py`), so a scene costs what drawing it
//! costs. It needs `fork`, and it needs to know which Python has manim
//! installed; when either is missing, the command is used instead.

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::Config;

const WORKER: &str = include_str!("worker.py");

/// How long the worker may take to import manim. The first import after
/// installing it compiles every module, which is slow.
const START_TIMEOUT: Duration = Duration::from_secs(60);

/// How often a render in progress checks whether it is still wanted.
const POLL: Duration = Duration::from_millis(15);

/// How one render ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// The frame manim wrote, as it wrote it.
    Frame(Vec<u8>),
    /// What went wrong, in a line.
    Failed(String),
    /// Nobody wants it any more.
    Cancelled,
}

/// What renders scenes.
pub(crate) enum Engine {
    Worker(Worker),
    Command,
}

impl Engine {
    /// The worker if it can be started, the command otherwise.
    pub(crate) fn start(config: &Config) -> Self {
        if cfg!(unix) && path_is_safe(&config.cache_dir) {
            if let Some(python) = python_for(config) {
                if let Ok(worker) = Worker::start(&python, &config.cache_dir) {
                    return Self::Worker(worker);
                }
            }
        }
        Self::Command
    }

    /// Renders `scene` from `work/scene.py`. `wanted` is asked every few
    /// milliseconds; once it says no, the render is stopped.
    pub(crate) fn render(
        &mut self,
        config: &Config,
        work: &Path,
        scene: &str,
        wanted: &dyn Fn() -> bool,
    ) -> Outcome {
        match self {
            Self::Worker(w) => w.render(config, work, scene, wanted),
            Self::Command => command(config, work, scene, wanted),
        }
    }

    /// Whether this engine can take another render. A worker that died
    /// cannot.
    pub(crate) fn alive(&mut self) -> bool {
        match self {
            Self::Worker(w) => w.child.try_wait().is_ok_and(|status| status.is_none()),
            Self::Command => true,
        }
    }
}

/// A Python process with manim imported, taking one request per line.
pub(crate) struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next: u64,
}

impl Worker {
    fn start(python: &Path, cache_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
        let log = cache_dir.join("worker.log");
        let stderr = fs::File::create(&log).map_err(|e| e.to_string())?;
        let mut child = Command::new(python)
            .args(["-u", "-c", WORKER])
            .env("PYTHONWARNINGS", "ignore")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", python.display()))?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (tx, lines) = mpsc::channel();
        std::thread::Builder::new()
            .name("manim-worker".to_owned())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let stdin = child.stdin.take();
        let mut worker = Self {
            child,
            stdin,
            lines,
            next: 0,
        };
        match worker.lines.recv_timeout(START_TIMEOUT) {
            Ok(line) if line.starts_with("ready\t") => Ok(worker),
            _ => {
                worker.stop();
                Err(last_error(&log))
            }
        }
    }

    fn render(
        &mut self,
        config: &Config,
        work: &Path,
        scene: &str,
        wanted: &dyn Fn() -> bool,
    ) -> Outcome {
        self.next += 1;
        let id = self.next.to_string();
        let request = format!(
            "render\t{id}\t{}\t{scene}\t{}\t{}\n",
            config.quality,
            work.display(),
            config.timeout.as_secs().max(1),
        );
        if !self.send(&request) {
            return Outcome::Failed("the manim worker stopped".to_owned());
        }
        let deadline = Instant::now() + config.timeout;
        loop {
            match self.lines.recv_timeout(POLL) {
                Ok(line) => match line.split_once('\t') {
                    Some(("done", rid)) if rid == id => {
                        return frame(work).map_or_else(Outcome::Failed, Outcome::Frame);
                    }
                    Some(("fail", rid)) if rid == id => {
                        let error = work.join("error.txt");
                        let why = if error.exists() {
                            last_error(&error)
                        } else {
                            // Killed before it could say: the alarm.
                            format!("stopped after {}s", config.timeout.as_secs())
                        };
                        return Outcome::Failed(why);
                    }
                    // An answer to a render given up on.
                    _ => {}
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Outcome::Failed("the manim worker stopped".to_owned());
                }
            }
            if !wanted() {
                self.send(&format!("cancel\t{id}\n"));
                return Outcome::Cancelled;
            }
            if Instant::now() > deadline {
                self.send(&format!("cancel\t{id}\n"));
                return Outcome::Failed(format!("stopped after {}s", config.timeout.as_secs()));
            }
        }
    }

    fn send(&mut self, line: &str) -> bool {
        self.stdin.as_mut().is_some_and(|s| {
            s.write_all(line.as_bytes())
                .and_then(|()| s.flush())
                .is_ok()
        })
    }

    fn stop(&mut self) {
        // Closing stdin tells the worker to stop its renders and go.
        self.stdin = None;
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One run of the `manim` command.
fn command(config: &Config, work: &Path, scene: &str, wanted: &dyn Fn() -> bool) -> Outcome {
    let Some((program, before)) = config.command.split_first() else {
        return Outcome::Failed("no manim command configured".to_owned());
    };
    let log = match fs::File::create(work.join("log.txt")) {
        Ok(f) => f,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let quality = format!("-q{}", config.quality);
    let spawned = Command::new(program)
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
            // No asking PyPI for a newer version first.
            "--silent",
            "scene.py",
            scene,
        ])
        .current_dir(work)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Outcome::Failed(format!(
                "{program} not found: install Manim (https://www.manim.community) or set [manim] command"
            ));
        }
        Err(e) => return Outcome::Failed(format!("cannot start {program}: {e}")),
    };
    let deadline = Instant::now() + config.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let stop = if !wanted() {
                    Some(Outcome::Cancelled)
                } else if Instant::now() > deadline {
                    Some(Outcome::Failed(format!(
                        "stopped after {}s",
                        config.timeout.as_secs()
                    )))
                } else {
                    None
                };
                if let Some(outcome) = stop {
                    let _ = child.kill();
                    let _ = child.wait();
                    return outcome;
                }
                std::thread::sleep(POLL);
            }
            Err(e) => return Outcome::Failed(e.to_string()),
        }
    };
    if !status.success() {
        return Outcome::Failed(last_error(&work.join("log.txt")));
    }
    frame(work).map_or_else(Outcome::Failed, Outcome::Frame)
}

/// The frame manim left under `work/media`. Where exactly depends on how
/// it was run, so it is looked for.
fn frame(work: &Path) -> Result<Vec<u8>, String> {
    fn find(dir: &Path, depth: usize) -> Option<PathBuf> {
        let entries = fs::read_dir(dir).ok()?;
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.file_name().is_some_and(|n| n == "frame.png") {
                return Some(path);
            }
        }
        if depth == 0 {
            return None;
        }
        dirs.into_iter().find_map(|d| find(&d, depth - 1))
    }
    let path = find(&work.join("media"), 4).ok_or("manim finished but left no frame")?;
    fs::read(path).map_err(|e| e.to_string())
}

/// The useful end of a log or traceback: the error, rather than the
/// whole story.
pub(crate) fn last_error(log: &Path) -> String {
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

/// The requests are tab-separated lines, so the paths in them cannot hold
/// either.
fn path_is_safe(path: &Path) -> bool {
    path.to_str().is_some_and(|p| !p.contains(['\t', '\n']))
}

/// The Python that has manim installed: the one set in the config, or the
/// one the `manim` command's own script names.
pub(crate) fn python_for(config: &Config) -> Option<PathBuf> {
    if let Some(python) = &config.python {
        return Some(python.clone());
    }
    match config.command.as_slice() {
        [program] => {
            let path = find_program(program)?;
            if is_python(&path) {
                return Some(path);
            }
            interpreter(&path)
        }
        // `python -m manim`
        [python, m, module] if m == "-m" && module == "manim" && is_python(Path::new(python)) => {
            Some(PathBuf::from(python))
        }
        _ => None,
    }
}

fn is_python(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("python"))
}

fn find_program(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_owned());
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

/// The interpreter a script's `#!` line names. Console scripts installed
/// by pip, pipx, uv and conda all name the Python they belong to.
fn interpreter(script: &Path) -> Option<PathBuf> {
    let mut head = [0u8; 512];
    let n = fs::File::open(script).ok()?.read(&mut head).ok()?;
    let text = std::str::from_utf8(&head[..n]).ok()?;
    let mut words = text.lines().next()?.strip_prefix("#!")?.split_whitespace();
    let program = words.next()?;
    if program.ends_with("/env") {
        return words.find(|w| !w.starts_with('-')).map(PathBuf::from);
    }
    if program.ends_with("/sh") {
        // pip's way round an interpreter path with spaces in it:
        //   '''exec' "/path/to/python" "$0" "$@"
        let line = text.lines().find_map(|l| l.strip_prefix("'''exec' "))?;
        let quoted = line.strip_prefix('"')?;
        return quoted.split_once('"').map(|(p, _)| PathBuf::from(p));
    }
    Some(PathBuf::from(program))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_python_comes_from_the_script() {
        let dir = tempfile::tempdir().expect("tempdir");
        let script = |name: &str, text: &str| {
            let path = dir.path().join(name);
            fs::write(&path, text).expect("write");
            interpreter(&path)
        };
        assert_eq!(
            script(
                "a",
                "#!/home/me/.local/share/uv/tools/manim/bin/python\nimport sys\n"
            ),
            Some(PathBuf::from(
                "/home/me/.local/share/uv/tools/manim/bin/python"
            ))
        );
        assert_eq!(
            script("b", "#!/usr/bin/env python3\n"),
            Some(PathBuf::from("python3"))
        );
        assert_eq!(
            script(
                "c",
                "#!/bin/sh\n'''exec' \"/Users/me/My Apps/venv/bin/python\" \"$0\" \"$@\"\n' '''\n"
            ),
            Some(PathBuf::from("/Users/me/My Apps/venv/bin/python"))
        );
        assert_eq!(script("d", "#!/bin/sh\necho hi\n"), None);
        assert_eq!(script("e", "no shebang\n"), None);
    }

    #[test]
    fn commands_that_hide_their_python_use_the_command() {
        let mut config = Config::new(PathBuf::from("/tmp/x"));
        config.command = vec!["uv".into(), "run".into(), "manim".into()];
        assert_eq!(python_for(&config), None);
        config.command = vec!["python3".into(), "-m".into(), "manim".into()];
        assert_eq!(python_for(&config), Some(PathBuf::from("python3")));
        config.python = Some(PathBuf::from("/opt/py"));
        assert_eq!(python_for(&config), Some(PathBuf::from("/opt/py")));
        assert!(!path_is_safe(Path::new("/tmp/a\tb")));
    }
}
