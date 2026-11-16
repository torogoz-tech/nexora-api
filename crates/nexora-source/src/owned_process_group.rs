use crate::SourceError;
use std::{
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus},
};

/// Keeps the leader unreaped until its group has been stopped, preventing PID-reuse signaling.
pub(crate) struct OwnedProcessGroup {
    pub child: Child,
    pid: libc::pid_t,
    reaped: bool,
}
impl OwnedProcessGroup {
    pub fn spawn(command: &mut Command, max_file_bytes: Option<u64>) -> Result<Self, SourceError> {
        if let Some(maximum) = max_file_bytes {
            // SAFETY: initialized C POD storage is passed to getrlimit in the parent.
            let mut inherited: libc::rlimit = unsafe { std::mem::zeroed() };
            if unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut inherited) } != 0 {
                return Err(SourceError::Unavailable);
            }
            #[allow(
                clippy::useless_conversion,
                reason = "rlim_t is narrower than u64 on some supported Unix architectures"
            )]
            let requested: libc::rlim_t = maximum
                .try_into()
                .map_err(|_| SourceError::InvalidConfiguration)?;
            let cap = requested.min(inherited.rlim_max);
            let limit = libc::rlimit {
                rlim_cur: cap,
                rlim_max: cap,
            };
            // SAFETY: after fork this closure only invokes the resource-limit syscall and
            // obtains an OS error code; it does not allocate, log, lock or invoke repository code.
            unsafe {
                command.pre_exec(move || {
                    if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) == 0 {
                        Ok(())
                    } else {
                        Err(std::io::Error::last_os_error())
                    }
                });
            }
        }
        let mut child = command
            .process_group(0)
            .spawn()
            .map_err(|_| SourceError::Unavailable)?;
        let pid = match i32::try_from(child.id()) {
            Ok(pid) if pid > 1 => pid,
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SourceError::Unavailable);
            }
        };
        Ok(Self {
            child,
            pid,
            reaped: false,
        })
    }
    pub fn exited(&self) -> Result<bool, SourceError> {
        // SAFETY: siginfo_t is a C POD structure. waitid receives its valid, initialized storage
        // and the positive PID of our owned child. WNOWAIT retains ownership until stop().
        let mut information: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.pid as libc::id_t,
                &mut information,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                return Ok(false);
            }
            return Err(SourceError::Unavailable);
        }
        Ok(information.si_signo != 0)
    }
    pub fn stop(&mut self) -> Result<ExitStatus, SourceError> {
        if self.reaped {
            return Err(SourceError::Unavailable);
        }
        // SAFETY: process_group(0) made this unreaped child's positive PID its dedicated group ID.
        // No signal is sent after wait() releases that identity.
        unsafe {
            libc::killpg(self.pid, libc::SIGKILL);
        }
        let result = self.child.wait().map_err(|_| SourceError::Unavailable);
        self.reaped = true;
        result
    }
}
impl Drop for OwnedProcessGroup {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.stop();
        }
    }
}
