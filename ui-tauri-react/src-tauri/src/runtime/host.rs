use std::ffi::OsStr;
use std::process::{Command, Output};

/// Narrow Interface for host process execution.
///
/// Production code uses [`ProcessCommandRunner`]; tests use [`FakeCommandRunner`]
/// so Modules can verify behavior without invoking Linux desktop/system services.
pub trait CommandRunner {
    fn output(&self, program: &str, args: &[&str]) -> Result<Output, String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn output(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        Command::new(program)
            .args(args.iter().map(OsStr::new))
            .output()
            .map_err(|e| format!("Failed to run {program}: {e}"))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};
    use std::sync::Mutex;

    #[derive(Debug)]
    pub struct FakeCommandRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
        outputs: Mutex<VecDeque<Result<Output, String>>>,
    }

    impl FakeCommandRunner {
        pub fn new(outputs: impl IntoIterator<Item = Result<Output, String>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                outputs: Mutex::new(outputs.into_iter().collect()),
            }
        }

        pub fn success(stdout: &str) -> Output {
            Output {
                status: ExitStatus::from_raw(0),
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
            }
        }

        pub fn failure(stderr: &str) -> Output {
            Output {
                status: ExitStatus::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: stderr.as_bytes().to_vec(),
            }
        }

        pub fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl CommandRunner for FakeCommandRunner {
        fn output(&self, program: &str, args: &[&str]) -> Result<Output, String> {
            self.calls
                .lock()
                .expect("calls lock")
                .push((program.to_string(), args.iter().map(|arg| arg.to_string()).collect()));
            self.outputs
                .lock()
                .expect("outputs lock")
                .pop_front()
                .unwrap_or_else(|| Err(format!("No fake output for {program}")))
        }
    }
}
