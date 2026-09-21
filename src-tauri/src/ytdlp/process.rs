//! Process ownership and interruptible reads for the download subprocess only.
use std::io::{self, Read};
#[cfg(any(windows, test))]
use std::process::Stdio;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub(super) fn configure_download_process(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

pub(super) fn terminate_process_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // The command was launched in its own group. SIGKILL also reaches
        // grandchildren and children that ignore TERM after their parent exits.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let _ = super::engine::silent_command("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

#[cfg(unix)]
use std::os::fd::AsRawFd as PipeHandle;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle as PipeHandle;

pub(super) struct PollingPipe<R> {
    pipe: R,
    stop: Arc<AtomicBool>,
    drain_remaining: Option<usize>,
    init_error: Option<io::Error>,
}

impl<R: Read + PipeHandle> PollingPipe<R> {
    pub(super) fn new(pipe: R, stop: Arc<AtomicBool>) -> Self {
        #[cfg(unix)]
        let init_error = unsafe {
            let fd = pipe.as_raw_fd();
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                Some(io::Error::last_os_error())
            } else {
                None
            }
        };
        #[cfg(windows)]
        let init_error = None;
        Self {
            pipe,
            stop,
            drain_remaining: None,
            init_error,
        }
    }
}

impl<R: Read + PipeHandle> Read for PollingPipe<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(error) = self.init_error.take() {
            return Err(error);
        }
        loop {
            if self.stop.load(Ordering::Acquire) && self.drain_remaining.is_none() {
                // Snapshot the queued bytes, not elapsed consumer time: progress
                // callbacks may be slow, while escaped writers must not extend EOF.
                #[cfg(unix)]
                let available = unsafe {
                    let mut available: libc::c_int = 0;
                    if libc::ioctl(self.pipe.as_raw_fd(), libc::FIONREAD, &mut available) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    available.max(0) as usize
                };
                #[cfg(windows)]
                let available = {
                    let mut available = 0;
                    let ok = unsafe {
                        windows_sys::Win32::System::Pipes::PeekNamedPipe(
                            self.pipe.as_raw_handle(),
                            std::ptr::null_mut(),
                            0,
                            std::ptr::null_mut(),
                            &mut available,
                            std::ptr::null_mut(),
                        )
                    };
                    if ok == 0 {
                        return Ok(0);
                    }
                    available as usize
                };
                self.drain_remaining = Some(available);
            }
            if self.drain_remaining == Some(0) {
                return Ok(0);
            }
            #[cfg(windows)]
            {
                let mut available = 0;
                let ok = unsafe {
                    windows_sys::Win32::System::Pipes::PeekNamedPipe(
                        self.pipe.as_raw_handle(),
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    return Ok(0);
                }
                if available == 0 {
                    if self.drain_remaining.is_some() {
                        return Ok(0);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                }
            }
            let limit = self
                .drain_remaining
                .unwrap_or(buffer.len())
                .min(buffer.len());
            match self.pipe.read(&mut buffer[..limit]) {
                Ok(count) => {
                    if let Some(remaining) = &mut self.drain_remaining {
                        *remaining -= count;
                    }
                    return Ok(count);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if self.drain_remaining.is_some() {
                        return Ok(0);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn kills_nested_term_ignoring_processes_and_releases_inherited_pipe() {
        let mut command = Command::new("sh");
        configure_download_process(&mut command);
        command
            .args(["-c", "sh -c 'trap \"\" TERM; sleep 30 & wait' & wait"])
            .stdout(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let pipe = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut pipe = pipe;
            let mut bytes = Vec::new();
            tx.send(pipe.read_to_end(&mut bytes)).unwrap();
        });
        std::thread::sleep(Duration::from_millis(100));
        terminate_process_tree(&mut child);
        assert!(!child.wait().unwrap().success());
        rx.recv_timeout(Duration::from_secs(2))
            .expect("descendants retained stdout")
            .unwrap();
        reader.join().unwrap();
    }

    #[test]
    fn cleans_descendants_after_parent_exits_and_drains_final_output() {
        let mut command = Command::new("sh");
        configure_download_process(&mut command);
        let mut child = command
            .args(["-c", "sleep 30 & printf 'output:final.mp4\\n'"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mut pipe = PollingPipe::new(child.stdout.take().unwrap(), Arc::clone(&stop));
        assert!(child.wait().unwrap().success());
        terminate_process_tree(&mut child);
        stop.store(true, Ordering::Release);
        let mut output = String::new();
        pipe.read_to_string(&mut output).unwrap();
        assert_eq!(output, "output:final.mp4\n");
    }

    #[test]
    fn draining_retains_tail_even_when_consumer_is_slow() {
        use std::io::Write;
        let (read, mut write) = std::os::unix::net::UnixStream::pair().unwrap();
        let expected = vec![b'x'; 16_384];
        write.write_all(&expected).unwrap();
        let stop = Arc::new(AtomicBool::new(true));
        let mut pipe = PollingPipe::new(read, stop);
        let mut prefix = [0; 8192];
        assert_eq!(pipe.read(&mut prefix).unwrap(), 8192);
        std::thread::sleep(Duration::from_millis(250));
        let mut tail = Vec::new();
        pipe.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, expected[8192..]);
    }

    #[test]
    fn polling_reader_stops_even_when_writer_keeps_pipe_open() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mut pipe = PollingPipe::new(child.stdout.take().unwrap(), Arc::clone(&stop));
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            tx.send(pipe.read_to_end(&mut Vec::new())).unwrap();
        });
        stop.store(true, Ordering::Release);
        let result = rx.recv_timeout(Duration::from_secs(2));
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(result.unwrap().unwrap(), 0);
        reader.join().unwrap();
    }
}
