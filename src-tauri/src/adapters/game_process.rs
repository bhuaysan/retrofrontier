use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Everything needed to start one managed RetroArch child.
///
/// The program is always an absolute authenticated path and the environment is always fully
/// constructed, so nothing here can be resolved through the host `PATH` or the user's own
/// RetroArch environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnRequest {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<String, String>,
    pub working_directory: PathBuf,
}

/// How a managed process ended, before RetroFrontier classifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExit {
    Code(i32),
    Signal(i32),
}

impl ProcessExit {
    pub fn code(self) -> Option<i64> {
        match self {
            Self::Code(code) => Some(i64::from(code)),
            Self::Signal(_) => None,
        }
    }

    pub fn is_clean(self) -> bool {
        matches!(self, Self::Code(0))
    }

    fn from_status(status: std::process::ExitStatus) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return Self::Signal(signal);
            }
        }
        Self::Code(status.code().unwrap_or(-1))
    }
}

/// Test-only fault injection for the managed-process boundary.
///
/// Uncertain-death handling has to be provable, and forcing the OS `waitpid`/`kill` calls to fail
/// is not something a test can do deterministically. Faults are injected here instead, through a
/// shared handle so a test can also lift them again while the process under test keeps running.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct ProcessFaults {
    inner: std::sync::Arc<std::sync::Mutex<Faults>>,
}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
struct Faults {
    wait: bool,
    try_exit: bool,
    terminate: bool,
}

#[cfg(test)]
impl ProcessFaults {
    pub fn fail_wait(&self, fail: bool) {
        self.inner.lock().expect("process fault lock").wait = fail;
    }

    pub fn fail_try_exit(&self, fail: bool) {
        self.inner.lock().expect("process fault lock").try_exit = fail;
    }

    pub fn fail_terminate(&self, fail: bool) {
        self.inner.lock().expect("process fault lock").terminate = fail;
    }

    fn injected(&self, select: impl Fn(&Faults) -> bool) -> Option<std::io::Error> {
        select(&self.inner.lock().expect("process fault lock"))
            .then(|| std::io::Error::other("injected managed-process failure"))
    }
}

/// A live managed child. Waiting happens on a blocking task, never on the UI thread.
#[derive(Debug)]
pub struct SpawnedGame {
    child: Child,
    #[cfg(test)]
    faults: ProcessFaults,
}

impl SpawnedGame {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Arm test-only failures on this child. Production code never calls this.
    #[cfg(test)]
    pub fn inject_faults(&mut self, faults: ProcessFaults) {
        self.faults = faults;
    }

    /// Has the child already ended? Used immediately after spawn so an emulator that fails at
    /// startup is reported as an early exit rather than as a broken process identity.
    pub fn try_exit(&mut self) -> Result<Option<ProcessExit>, std::io::Error> {
        #[cfg(test)]
        if let Some(error) = self.faults.injected(|faults| faults.try_exit) {
            return Err(error);
        }
        Ok(self.child.try_wait()?.map(ProcessExit::from_status))
    }

    /// Stop the child and reap it.
    ///
    /// This runs when process identity could not be established or durably recorded. Leaving the
    /// child alive would make it invisible to RuntimeManager's safety checks.
    ///
    /// `Ok` is a positive observation that the child has exited and been reaped, and it also says
    /// *how* it ended. That distinction has to come from this single reaping observation: asking
    /// separately beforehand whether the child had already exited races the child itself, and a
    /// caller that lost the race reported an instant exit as a failure to establish identity.
    ///
    /// `Err` proves nothing about the child, so a caller must keep treating it as possibly alive.
    pub fn terminate(&mut self) -> Result<Termination, std::io::Error> {
        #[cfg(test)]
        if let Some(error) = self.faults.injected(|faults| faults.terminate) {
            return Err(error);
        }
        match self.child.try_wait()? {
            Some(status) => Ok(Termination {
                exit: ProcessExit::from_status(status),
                exited_on_its_own: true,
            }),
            None => {
                self.child.kill()?;
                let exit = ProcessExit::from_status(self.child.wait()?);
                // A child that was alive a moment ago can still exit before the signal reaches
                // it, so *when* the exit was observed proves nothing. The reaped status does: a
                // normal exit code can only have come from the child itself, and the only signal
                // this function ever sends is `SIGKILL`, so that is the one status that means
                // termination — rather than the game — ended it.
                let exited_on_its_own = !matches!(exit, ProcessExit::Signal(TERMINATION_SIGNAL));
                Ok(Termination {
                    exit,
                    exited_on_its_own,
                })
            }
        }
    }

    /// Block until the child ends. The caller runs this on a blocking task.
    ///
    /// The handle is borrowed rather than consumed so that a failed wait — which proves nothing
    /// about the child — leaves the caller able to keep re-checking it.
    pub fn wait(&mut self) -> Result<ProcessExit, std::io::Error> {
        #[cfg(test)]
        if let Some(error) = self.faults.injected(|faults| faults.wait) {
            return Err(error);
        }
        Ok(ProcessExit::from_status(self.child.wait()?))
    }
}

/// The signal [`SpawnedGame::terminate`] sends, and therefore the only status that can mean
/// RetroFrontier ended the child rather than the child ending on its own.
const TERMINATION_SIGNAL: i32 = 9;

/// How a managed child ended when it was stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Termination {
    pub exit: ProcessExit,
    /// The child was already gone when termination ran, so nothing signalled it.
    pub exited_on_its_own: bool,
}

pub trait GameProcessLauncher: Send + Sync {
    fn spawn(&self, request: &SpawnRequest) -> Result<SpawnedGame, std::io::Error>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxGameProcessLauncher;

impl GameProcessLauncher for LinuxGameProcessLauncher {
    fn spawn(&self, request: &SpawnRequest) -> Result<SpawnedGame, std::io::Error> {
        if !request.program.is_absolute() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the managed runtime executable must be an absolute path",
            ));
        }

        let mut command = Command::new(&request.program);
        command
            .args(&request.arguments)
            .current_dir(&request.working_directory)
            // The environment is composed, never inherited: clearing first is what makes the
            // allowlist authoritative.
            .env_clear()
            .envs(&request.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn()?;
        Ok(SpawnedGame {
            child,
            #[cfg(test)]
            faults: ProcessFaults::default(),
        })
    }
}

/// Absolute path helper for building a spawn request.
pub fn absolute(path: &Path) -> Result<PathBuf, std::io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a launch path must be absolute",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{GameProcessLauncher, LinuxGameProcessLauncher, ProcessExit, SpawnRequest};
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use tempfile::{tempdir, TempDir};

    /// A synthetic managed executable. CI never needs a real emulator for lifecycle coverage.
    fn synthetic_runtime(script: &str) -> (TempDir, PathBuf) {
        let directory = tempdir().unwrap();
        let program = directory.path().join("AppRun");
        std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        (directory, program)
    }

    fn request(program: &Path, working_directory: &Path, arguments: &[&str]) -> SpawnRequest {
        SpawnRequest {
            program: program.to_path_buf(),
            arguments: arguments.iter().map(OsString::from).collect(),
            environment: BTreeMap::from([
                ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
                ("RF_MARKER".to_owned(), "controlled".to_owned()),
            ]),
            working_directory: working_directory.to_path_buf(),
        }
    }

    /// Spawning a freshly written executable can transiently fail with `ETXTBSY` when a parallel
    /// test thread forked while the writing descriptor was open. Test-harness concern only.
    fn spawn_with_retry(request: &SpawnRequest) -> super::SpawnedGame {
        for _ in 0..50 {
            match LinuxGameProcessLauncher.spawn(request) {
                Ok(child) => return child,
                Err(error) if error.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(error) => panic!("synthetic runtime should spawn: {error}"),
            }
        }
        panic!("synthetic runtime should spawn before the retry budget is exhausted");
    }

    #[test]
    fn a_clean_exit_and_a_non_zero_exit_are_distinct_normalized_results() {
        let (directory, program) = synthetic_runtime("exit \"$1\"");

        let mut clean = spawn_with_retry(&request(&program, directory.path(), &["0"]));
        assert_eq!(clean.wait().unwrap(), ProcessExit::Code(0));

        let mut failed = spawn_with_retry(&request(&program, directory.path(), &["3"]));
        let exit = failed.wait().unwrap();
        assert_eq!(exit, ProcessExit::Code(3));
        assert!(!exit.is_clean());
        assert_eq!(exit.code(), Some(3));
    }

    #[test]
    fn a_terminated_child_is_reported_as_a_signal_without_an_exit_code() {
        let (directory, program) = synthetic_runtime("sleep 30; :");
        let mut child = spawn_with_retry(&request(&program, directory.path(), &[]));
        assert!(child.pid() > 0);

        let termination = child.terminate().unwrap();

        assert!(matches!(termination.exit, ProcessExit::Signal(_)));
        assert_eq!(termination.exit.code(), None);
        assert!(!termination.exit.is_clean());
        // It had to be killed, so it did not end on its own.
        assert!(!termination.exited_on_its_own);
    }

    #[test]
    fn a_child_that_exits_immediately_is_observable_without_waiting() {
        let (directory, program) = synthetic_runtime("exit 7");
        let mut child = spawn_with_retry(&request(&program, directory.path(), &[]));

        let mut observed = None;
        for _ in 0..100 {
            if let Some(exit) = child.try_exit().unwrap() {
                observed = Some(exit);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_eq!(observed, Some(ProcessExit::Code(7)));
        // Terminating an already-finished child reports the same result instead of failing, and
        // says the child ended on its own rather than because termination signalled it.
        let termination = child.terminate().unwrap();
        assert_eq!(termination.exit, ProcessExit::Code(7));
        assert!(termination.exited_on_its_own);
    }

    #[test]
    fn the_child_receives_only_the_constructed_environment() {
        let (directory, program) =
            synthetic_runtime("env > \"$1\"; test -n \"$RF_MARKER\" || exit 9");
        std::env::set_var("RF_HOST_ONLY", "leaked");
        let output = directory.path().join("environment.txt");

        let mut child = spawn_with_retry(&request(
            &program,
            directory.path(),
            &[output.to_str().unwrap()],
        ));
        assert_eq!(child.wait().unwrap(), ProcessExit::Code(0));

        let observed = std::fs::read_to_string(&output).unwrap();
        assert!(observed.contains("RF_MARKER=controlled"));
        assert!(!observed.contains("RF_HOST_ONLY"));
        std::env::remove_var("RF_HOST_ONLY");
    }

    #[test]
    fn a_relative_or_missing_program_is_a_spawn_failure() {
        let directory = tempdir().unwrap();

        let relative = SpawnRequest {
            program: PathBuf::from("retroarch"),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
            working_directory: directory.path().to_path_buf(),
        };
        let error = LinuxGameProcessLauncher.spawn(&relative).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);

        let missing = request(
            &directory.path().join("absent").join("AppRun"),
            directory.path(),
            &[],
        );
        assert!(LinuxGameProcessLauncher.spawn(&missing).is_err());
    }
}
