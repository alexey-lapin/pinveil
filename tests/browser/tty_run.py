"""Runs a command on a pseudo-terminal and answers its prompts. Dev-only test helper.

The pinveil CLI reads the link and PIN at hidden prompts on the controlling terminal, and
Node has no pseudo-terminal of its own, so the browser tests drive the CLI through this.

    python3 tty_run.py [--stdin FILE] [--stdout FILE] [--answer PROMPT REPLY]... -- CMD ARGS...

Each --answer waits for PROMPT on the terminal, then types REPLY and Enter, in order.
Prints JSON: {"exit": code, "terminal": everything the terminal showed}.
"""

import json
import os
import pty
import select
import sys
import time

DEADLINE = 60


def main(argv):
    stdin = stdout = None
    answers = []
    i = 0
    while argv[i] != "--":
        if argv[i] == "--stdin":
            stdin, i = argv[i + 1], i + 2
        elif argv[i] == "--stdout":
            stdout, i = argv[i + 1], i + 2
        elif argv[i] == "--answer":
            answers.append((argv[i + 1].encode(), argv[i + 2].encode())),
            i += 3
        else:
            raise SystemExit(f"unknown option {argv[i]}")
    command = argv[i + 1 :]

    pid, fd = pty.fork()
    if pid == 0:
        # The pty stays the controlling terminal; only the standard streams are redirected.
        if stdin:
            os.dup2(os.open(stdin, os.O_RDONLY), 0)
        if stdout:
            os.dup2(os.open(stdout, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), 1)
        os.execvp(command[0], command)

    shown = b""
    searched = 0
    deadline = time.monotonic() + DEADLINE
    while time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            try:
                chunk = os.read(fd, 4096)
            except OSError:
                break
            if not chunk:
                break
            shown += chunk
        if answers:
            at = shown.find(answers[0][0], searched)
            if at != -1:
                searched = at + len(answers[0][0])
                os.write(fd, answers.pop(0)[1] + b"\n")
    else:
        os.kill(pid, 9)
    _, status = os.waitpid(pid, 0)
    print(json.dumps({"exit": os.waitstatus_to_exitcode(status), "terminal": shown.decode("utf-8", "replace")}))


if __name__ == "__main__":
    main(sys.argv[1:])
