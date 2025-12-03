use {
    nix::{
        errno::Errno,
        sys::signal::{Signal, killpg},
        unistd::{Pid, setpgid},
    },
    serde::{Deserialize, Deserializer, Serialize, Serializer},
    std::{
        ffi::OsStr,
        io,
        os::unix::process::CommandExt,
        process::{Child, Command, Stdio},
        sync::Arc,
    },
};

/// Spawns a process in its own process group and returns the Child process and its PGID.
pub fn spawn_pg<I>(cmd: &str, args: &[I]) -> io::Result<(Child, i32)>
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
                .map_err(|e| io::Error::from_raw_os_error(e as i32))
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

pub fn deserialize_arc<'de, D, T>(deserializer: D) -> Result<Arc<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let value = T::deserialize(deserializer)?;
    Ok(Arc::new(value))
}

pub fn serialize_arc<T, S>(value: &Arc<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    value.serialize(serializer)
}
