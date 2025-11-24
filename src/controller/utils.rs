use {
    nix::{
        errno::Errno,
        sys::signal::{Signal, killpg},
        unistd::{Pid, setpgid},
    },
    std::{
        ffi::OsStr,
        io::{Error, Result},
        os::unix::process::CommandExt,
        process::{Child, Command, Stdio},
    },
};

/// Spawns a process in its own process group and returns the Child process and its PGID.
pub fn spawn_pg<I>(cmd: &str, args: &[I]) -> Result<(Child, i32)>
where
    I: AsRef<OsStr>,
{
    let mut command = Command::new("stdbuf");

    command
        .arg("-oL")
        .arg(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set the process group ID to the child's PID
    unsafe {
        command.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|e| Error::from_raw_os_error(e as i32))
        });
    }

    let child = command.spawn()?;
    let pgid = child.id() as i32;

    Ok((child, pgid))
}

/// Kills a process group given its PGID. If `force` is true,
/// sends SIGKILL; otherwise, sends SIGTERM first.
pub fn kill_pg(pgid: i32, force: bool) -> nix::Result<()> {
    let pgid = Pid::from_raw(pgid);

    let signal = if force {
        Signal::SIGKILL
    } else {
        Signal::SIGTERM
    };
    match killpg(pgid, signal) {
        Ok(()) => Ok(()),

        Err(Errno::ESRCH) => {
            // Process group does not exist
            Ok(())
        }

        Err(e) if !force => {
            tracing::warn!("SIGTERM failed: {e}, falling back to SIGKILL");
            killpg(pgid, Signal::SIGKILL).or_else(|e| {
                if e == Errno::ESRCH {
                    // Process group does not exist
                    return Ok(());
                }
                Err(e)
            })
        }
        Err(e) => Err(e),
    }
}
