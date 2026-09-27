#!/usr/bin/env python3
"""Verify keyboard input works in droid_tui."""
import fcntl, os, pty, select, signal, struct, termios, time, sys

def set_size(fd, rows, cols):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', rows, cols, 0, 0))

def drain(fd, timeout=0.5):
    """Read all available output from fd with timeout."""
    data = b""
    while True:
        r, _, _ = select.select([fd], [], [], timeout)
        if not r:
            break
        try:
            chunk = os.read(fd, 4096)
            if not chunk:
                break
            data += chunk
        except OSError:
            break
    return data

pid, master_fd = pty.fork()
if pid == 0:
    os.execvp('./target/debug/droid_tui', ['./target/debug/droid_tui'])
else:
    set_size(master_fd, 30, 100)
    time.sleep(1.0)
    
    # Capture initial state
    initial = drain(master_fd, timeout=1.0)
    print(f"Initial output: {len(initial)} bytes")
    
    # Send 'l' to open file picker
    os.write(master_fd, b'l')
    time.sleep(0.5)
    after_l = drain(master_fd, timeout=0.5)
    print(f"After 'l': {len(after_l)} bytes")
    
    # Send 'q' to quit
    os.write(master_fd, b'q')
    time.sleep(0.5)
    after_q = drain(master_fd, timeout=0.5)
    print(f"After 'q': {len(after_q)} bytes")
    
    # Check if process exited
    time.sleep(0.5)
    try:
        os.kill(pid, 0)
        print("Process still running after 'q' - keys may not be working")
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        print("Process exited after 'q' - keys are working!")
    
    os.waitpid(pid, 0)
    os.close(master_fd)
