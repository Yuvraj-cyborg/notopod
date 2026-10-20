# notopod's manim worker.
#
# Started once, it pays for `import manim` up front and then renders each
# scene in a forked copy of itself, so a render starts with manim already
# loaded and anything a scene changes dies with its process.
#
# One request per line on stdin, fields separated by tabs:
#   render  ID  QUALITY  SCENE  DIR  SECONDS
#   cancel  ID
# DIR holds scene.py; the last frame is written under DIR/media.
# Answers on stdout:
#   ready  VERSION
#   done   ID
#   fail   ID          (DIR/error.txt says why)

import os
import select
import signal
import sys
import traceback
import warnings

warnings.filterwarnings("ignore")

import manim  # noqa: E402
from manim import config  # noqa: E402

QUALITIES = {"l": "low_quality", "m": "medium_quality", "h": "high_quality"}


def say(*fields):
    sys.stdout.write("\t".join(fields) + "\n")
    sys.stdout.flush()


def render(quality, scene, seconds):
    """Runs in the child. Never returns."""
    # Whatever happens to the parent, a scene cannot run for ever.
    signal.alarm(max(1, seconds))
    log = os.open("log.txt", os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    os.dup2(log, 1)
    os.dup2(log, 2)
    try:
        config.quality = QUALITIES.get(quality, "medium_quality")
        config.media_dir = "media"
        config.output_file = "frame"
        config.format = "png"
        config.save_last_frame = True
        config.transparent = True
        config.disable_caching = True
        config.progress_bar = "none"
        config.verbosity = "ERROR"
        with open("scene.py", encoding="utf-8") as f:
            code = compile(f.read(), "scene.py", "exec")
        names = {"__name__": "__notopod__"}
        exec(code, names)
        names[scene]().render()
        os._exit(0)
    except BaseException:
        with open("error.txt", "w", encoding="utf-8") as f:
            f.write(traceback.format_exc())
        os._exit(1)


def main():
    running = {}  # pid -> request id
    buffered = b""
    stdin = sys.stdin.fileno()
    say("ready", manim.__version__)
    while True:
        readable, _, _ = select.select([stdin], [], [], 0.02)
        if readable:
            chunk = os.read(stdin, 65536)
            if not chunk:
                # notopod is gone.
                for pid in running:
                    os.kill(pid, signal.SIGKILL)
                return
            buffered += chunk
            *lines, buffered = buffered.split(b"\n")
            for line in lines:
                fields = line.decode("utf-8", "replace").split("\t")
                if fields[0] == "render" and len(fields) == 6:
                    _, rid, quality, scene, directory, seconds = fields
                    pid = os.fork()
                    if pid == 0:
                        os.chdir(directory)
                        render(quality, scene, int(seconds))
                    running[pid] = rid
                elif fields[0] == "cancel" and len(fields) == 2:
                    for pid, rid in list(running.items()):
                        if rid == fields[1]:
                            os.kill(pid, signal.SIGKILL)
        while running:
            pid, status = os.waitpid(-1, os.WNOHANG)
            if pid == 0:
                break
            rid = running.pop(pid, None)
            if rid is not None:
                ok = os.WIFEXITED(status) and os.WEXITSTATUS(status) == 0
                say("done" if ok else "fail", rid)


if __name__ == "__main__":
    main()
