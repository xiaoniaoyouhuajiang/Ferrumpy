// Copyright 2020 The Evcxr Authors.
//
// Licensed under the Apache License, Version 2.0 <LICENSE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE
// or https://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::errors::Error;
use crate::errors::bail;
use crate::runtime;
use std::io::BufReader;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;

pub(crate) struct ChildProcess {
    process_handle: Arc<Mutex<std::process::Child>>,
    /// Whether cleanup of `process_handle` is the responsibility of another
    /// instance.
    process_disowned: bool,
    // Only none while in drop.
    stdin: Option<std::process::ChildStdin>,
    command: Arc<Mutex<process::Command>>,
    stdout_receiver: crossbeam_channel::Receiver<String>,
    stderr_sender: Arc<Mutex<crossbeam_channel::Sender<String>>>,
}

impl ChildProcess {
    pub(crate) fn new(
        mut command: std::process::Command,
        stderr_sender: crossbeam_channel::Sender<String>,
    ) -> Result<(ChildProcess, crossbeam_channel::Receiver<String>), Error> {
        // Avoid a fork bomb. We could call runtime_hook here but then all the work that we did up
        // to this point would be wasted. Also, it's possible that we could already have started
        // threads, which could get messy.
        if std::env::var(runtime::EVCXR_IS_RUNTIME_VAR).is_ok() {
            bail!("Our current binary doesn't call runtime_hook()");
        }
        // All arguments after -- get passed to the subprocess, which the user can then access via
        // std::env::args.
        let user_args = std::env::args().skip_while(|arg| arg != "--").skip(1);
        command
            .args(user_args)
            .env(runtime::EVCXR_IS_RUNTIME_VAR, "1")
            .env("RUST_BACKTRACE", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Create stdout channel internally
        let (stdout_sender, stdout_receiver) = crossbeam_channel::unbounded();

        let child_process = ChildProcess::new_internal(
            Arc::new(Mutex::new(command)),
            None,
            stdout_sender,
            stdout_receiver.clone(),
            Arc::new(Mutex::new(stderr_sender)),
        )?;

        Ok((child_process, stdout_receiver))
    }

    fn new_internal(
        command: Arc<Mutex<std::process::Command>>,
        process_handle: Option<Arc<Mutex<std::process::Child>>>,
        stdout_sender: crossbeam_channel::Sender<String>,
        stdout_receiver: crossbeam_channel::Receiver<String>,
        stderr_sender: Arc<Mutex<crossbeam_channel::Sender<String>>>,
    ) -> Result<ChildProcess, Error> {
        let process = command.lock().unwrap().spawn();
        let mut process = match process {
            Ok(c) => c,
            Err(error) => bail!("Failed to run '{:?}': {:?}", command, error),
        };

        let stdin = process.stdin.take();
        // Handle stderr and stdout by patching them through to channels in our output struct.
        let mut child_stderr =
            std::io::BufRead::lines(BufReader::new(process.stderr.take().unwrap()));
        let mut child_stdout =
            std::io::BufRead::lines(BufReader::new(process.stdout.take().unwrap()));

        // If we already have an Arc<Mutex<>> wrapping an old process, then
        // reuse it, putting our new process into it. If we don't, then create a
        // new Arc<Mutex<>>. It's important to reuse an existing Arc<Mutex<>>,
        // in order to uphold the guarantees of the process_handle method.
        let process_handle = match process_handle {
            Some(handle) => {
                core::mem::swap(&mut *handle.lock().unwrap(), &mut process);
                // Ensure the old process is properly cleaned up.
                let _ = process.wait();
                handle
            }
            None => Arc::new(Mutex::new(process)),
        };

        // Spawn background thread to drain stderr
        std::thread::spawn({
            let stderr_sender = Arc::clone(&stderr_sender);
            move || {
                let stderr_sender = stderr_sender.lock().unwrap();
                while let Some(Ok(line)) = child_stderr.next() {
                    // Ignore errors, since it just means that the user of the library has dropped the receive end.
                    let _ = stderr_sender.send(line);
                }
            }
        });

        // Spawn background thread to drain stdout (FIX: was missing, causing pipe blocking)
        std::thread::spawn({
            move || {
                while let Some(Ok(line)) = child_stdout.next() {
                    // Ignore errors, since it just means that the user of the library has dropped the receive end.
                    let _ = stdout_sender.send(line);
                }
            }
        });

        Ok(ChildProcess {
            process_handle,
            process_disowned: false,
            stdin,
            command,
            stdout_receiver,
            stderr_sender,
        })
    }

    /// Returns a handle to our subprocess. This handle may be used to terminate
    /// the subprocess. The returned handle is valid across restarts. i.e. if
    /// restart is called, then there is no need to call this method again to
    /// get a fresh handle - the previously returned handle will work for the
    /// new subprocess.
    pub(crate) fn process_handle(&self) -> Arc<Mutex<std::process::Child>> {
        self.process_handle.clone()
    }

    /// Terminates this process if it hasn't already, then restarts
    pub(crate) fn restart(&mut self) -> Result<ChildProcess, Error> {
        // If the process hasn't already terminated for some reason, kill it.
        let mut process = self.process_handle.lock().unwrap();
        if let Ok(None) = process.try_wait() {
            let _ = process.kill();
            let _ = process.wait();
        }
        self.process_disowned = true;
        // Unlock mutex, since ChildProcess::new_internal will need to lock it
        // again.
        drop(process);
        // Create new stdout channel for the restarted process
        let (new_stdout_sender, new_stdout_receiver) = crossbeam_channel::unbounded();

        ChildProcess::new_internal(
            Arc::clone(&self.command),
            Some(self.process_handle.clone()),
            new_stdout_sender,
            new_stdout_receiver,
            Arc::clone(&self.stderr_sender),
        )
    }

    pub(crate) fn send(&mut self, command: &str) -> Result<(), Error> {
        use std::io::Write;
        writeln!(self.stdin.as_mut().unwrap(), "{command}")
            .map_err(|_| self.get_termination_error())?;
        self.stdin.as_mut().unwrap().flush()?;
        Ok(())
    }

    /// Receive a line from the subprocess stdout
    pub(crate) fn recv_line(&mut self) -> Result<String, Error> {
        // Read from the stdout channel (which is being filled by background thread)
        self.stdout_receiver
            .recv()
            .map_err(|_| self.get_termination_error())
    }

    fn get_termination_error(&mut self) -> Error {
        // Wait until the stderr handling thread has released its lock on stderr_sender, which it
        // will do when there's nothing more to read from stderr. We don't need to keep the lock,
        // just wait until we can aquire it, then drop it straight away.
        std::mem::drop(self.stderr_sender.lock().unwrap());

        // Note: We can't read from stdout anymore since it's being drained by background thread.
        // The output has already been sent to the stdout channel. This is fine for error reporting
        // since users should be reading from the channel anyway.

        Error::SubprocessTerminated(match self.process_handle.lock().unwrap().wait() {
            Ok(exit_status) => {
                #[cfg(target_os = "macos")]
                {
                    use std::os::unix::process::ExitStatusExt;
                    if Some(9) == exit_status.signal() {
                        return Error::SubprocessTerminated(
                            "Subprocess terminated with signal 9. This is known \
                            to happen when evcxr is installed via a Homebrew shell \
                            under emulation. Try installing rustup and evcxr without \
                            using Homebrew and see if that helps."
                                .to_owned(),
                        );
                    }
                }
                format!("Subprocess terminated with status: {}", exit_status)
            }
            Err(err) => format!("Error waiting for subprocess: {}", err),
        })
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        // Drop child_stdin before we wait. Our subprocess uses stdin being
        // closed to know that it's time to terminate.
        self.stdin.take();
        if !self.process_disowned {
            // Wait for our subprocess to terminate. Otherwise we'll be left
            // with zombie processes.
            let _ = self.process_handle.lock().unwrap().wait();
        }
    }
}
