---
name: verify
description: Build and drive the droid_tui binary interactively to verify a change. Use when asked to run/verify/smoke-test the TUI rather than just run its test suite.
---

# Verifying droid_tui

Build first: `cargo build`.

## Driving it interactively

`tmux` is not installed in this environment; use `screen` instead (same idea,
different flags).

```bash
screen -dmS verify1 -O bash -c "COLUMNS=100 LINES=30 ./target/debug/droid_tui; echo EXIT_CODE_\$?; sleep 30"
sleep 1 && screen -S verify1 -X width 100 30   # only affects the *initial* size, see caveat below
screen -S verify1 -X stuff "l"                  # send keys (raw, no Enter appended)
screen -S verify1 -X stuff $'\r'                # send Enter
screen -S verify1 -X stuff $'\e'                # send Esc
screen -S verify1 -X stuff $'\x03'               # send Ctrl+C
screen -S verify1 -X hardcopy /path/to/out.txt && cat /path/to/out.txt   # capture the pane
screen -S verify1 -X quit                        # tear down when done
screen -wipe                                     # clear dead session entries
```

To load the bundled fixture patch from the empty state: `l`, then `j` x17 to
reach `fixtures`, `Enter`, `j` to reach `arpeggio1.ini`, `Enter`.

**Gotcha — rapid key bursts need real wait time.** Sending 20+ keys via one
`stuff` call and capturing after only 0.3s can show a stale, partially-applied
state (looked like a clamping bug once; it wasn't — just hadn't finished
processing). Wait ~1s after a burst of 15+ keys before capturing.

**Gotcha — `screen -X width` does not deliver `SIGWINCH` to the child in this
headless environment.** Confirmed with a throwaway Python SIGWINCH-logging
process: resizing the screen window live has no effect on the running child,
so you can't verify live terminal-resize reflow this way. Use the pty-based
probe instead (see below) — it drives a real pty directly and reliably
delivers the resize.

## Verifying live resize (bypasses screen)

```python
import fcntl, os, pty, select, signal, struct, termios, time

def set_size(fd, rows, cols):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', rows, cols, 0, 0))

pid, master_fd = pty.fork()
if pid == 0:
    os.execvp('./target/debug/droid_tui', ['./target/debug/droid_tui'])
else:
    set_size(master_fd, 30, 100)
    time.sleep(0.3)
    # ... os.write(master_fd, b'...') to drive it into the state you want to resize ...
    set_size(master_fd, 15, 40)
    os.kill(pid, signal.SIGWINCH)
    time.sleep(0.5)
    # drain master_fd with select(..., timeout) — NOT a blocking os.read loop,
    # which hangs forever once the buffer is empty.
```

Confirms reflow by checking the raw escape-sequence output for cursor
positions bounded by the new size (e.g. no `;100H` after resizing to 40 wide).

## Mouse input

`screen -X stuff` cannot send real mouse events. Inject raw SGR mouse escape
sequences instead via `screen -X readbuf <file> && screen -X paste .`:

```bash
printf '\x1b[<0;COL;ROWM\x1b[<0;COL;ROWm' > click.bin   # left click (press+release) at COL,ROW (1-indexed)
printf '\x1b[<64;COL;ROWM' > scrollup.bin                # scroll up at COL,ROW
printf '\x1b[<65;COL;ROWM' > scrolldown.bin               # scroll down
screen -S verify1 -X readbuf click.bin
screen -S verify1 -X paste .
```

Row/col math: header is 3 rows (border/title/border), so the first controller
panel's border starts at row 4, its first component row's label line is row
5, state line row 6. Each component cell is 16 columns wide, starting at
column 2 (after the left panel border).

## Known gaps (confirmed bugs, not test artifacts)

- **Ctrl+C does nothing while the file picker is open.** `handler::handle_event`
  checks `app.showing_picker` before the `q`/Ctrl+C quit arms, and
  `handle_picker_event` has no quit case — only `Esc` (close picker) works
  there. Confirmed via byte-identical pane hardcopy before/after Ctrl+C while
  the picker was showing.
