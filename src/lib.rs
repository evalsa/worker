use std::{
    ffi::OsString,
    path::Path,
    process::{Command, ExitStatus},
};

/// Options provided for compiling binaries.
struct CompileOption {
    /// File to execute when compile.
    pub compiler: OsString,
    /// Arguments passed to compiler process.
    pub args: Vec<OsString>,
}

/// Result of compilation.
struct CompileResult {
    /// Exit status of compiler process.
    pub status: ExitStatus,
    /// Output from standard error stream of compiler process.
    pub stderr: Vec<u8>,
    /// Output from standard output stream of compiler process.
    pub stdout: Vec<u8>,
}

/// Compiles source inside directory with options.
///
/// Returns a `CompileResult` object containing success flag and standard error stream data.
fn compile_at(directory: &Path, option: &CompileOption) -> std::io::Result<CompileResult> {
    Command::new(&option.compiler)
        .current_dir(directory)
        .args(option.args.iter().map(OsString::as_os_str))
        .output()
        .map(|output| CompileResult {
            status: output.status,
            stderr: output.stderr,
            stdout: output.stdout,
        })
}

/// Options provided for binary launch.
struct LaunchOption {
    /// Path to binary to launch.
    pub binary: OsString,
    /// Arguments passed to the binary.
    pub args: Vec<OsString>,
    /// Standard input passed to the binary.
    pub stdin: Vec<u8>,
    /// Maximum time for the binary to execute, in seconds.
    pub time: usize,
    /// Maximum virtual memory size, in MiB.
    pub virtual_memory: usize,
    /// Maximum size of files created by the binary, in MiB.
    pub files_size: usize,
    /// Maximum number of processes created by the binary.
    pub proc_count: usize,
    /// If `true`, mount procfs to `/proc`.
    pub mount_proc: bool,
    /// Kafel seccomp-bpf policy to use.
    pub seccomp: Option<String>,
}

/// Result of binary launch.
struct LaunchResult {
    /// Exit status of binary.
    pub status: ExitStatus,
    /// Output from standard error stream of binary.
    pub stderr: Vec<u8>,
    /// Output from standard output stream of binary.
    pub stdout: Vec<u8>,
}

/// Launches a binary with given options in nsjail, change root to `directory`.
fn launch(nsjail: &Path, directory: &Path, option: &LaunchOption) -> std::io::Result<LaunchResult> {
    let time = option.time.to_string();
    let virtual_memory = option.virtual_memory.to_string();
    let files_size = option.files_size.to_string();
    let proc_count = option.proc_count.to_string();
    let mut command = Command::new(nsjail);
    command
        .args(["-M", "e"]) // use execve
        .args(["-c".as_ref(), directory.as_os_str()]) // chroot to `directory`
        .args(["-H", "worker"]) // set hostname
        .args(["-t", &time])
        .args(["--rlimit_as", &virtual_memory])
        .args(["--rlimit_cpu", &time])
        .args(["--rlimit_fsize", &files_size])
        .args(["--rlimit_nproc", &proc_count]);
    if !option.mount_proc {
        command.arg("--disable_proc");
    }
    if let Some(seccomp) = &option.seccomp {
        command.arg("--seccomp_string").arg(seccomp);
    }
    command.output().map(|output| LaunchResult {
        status: output.status,
        stderr: output.stderr,
        stdout: output.stdout,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn compile_return_zero_succeeds() {
        let current = Path::new("./");
        let option = CompileOption {
            compiler: "/usr/sbin/true".into(),
            args: vec![],
        };
        let result = compile_at(current, &option).unwrap();
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        assert!(result.stdout.is_empty());
    }

    #[test]
    fn compile_return_one_fails() {
        let current = Path::new("./");
        let option = CompileOption {
            compiler: "/usr/sbin/false".into(),
            args: vec![],
        };
        let result = compile_at(current, &option).unwrap();
        assert!(!result.status.success());
        assert!(result.stderr.is_empty());
        assert!(result.stdout.is_empty());
    }

    #[test]
    fn compile_get_stderr() {
        let current = Path::new("./");
        let option = CompileOption {
            compiler: "/usr/bin/bash".into(),
            args: vec!["-c".into(), "echo dummy_stderr 1>&2".into()],
        };
        let result = compile_at(current, &option).unwrap();
        assert!(result.status.success());
        assert_eq!(&result.stderr, b"dummy_stderr\n");
        assert!(result.stdout.is_empty());
    }

    #[test]
    fn compile_get_stdout() {
        let current = Path::new("./");
        let option = CompileOption {
            compiler: "/usr/bin/bash".into(),
            args: vec!["-c".into(), "echo dummy_stdout".into()],
        };
        let result = compile_at(current, &option).unwrap();
        assert!(result.status.success());
        assert_eq!(&result.stdout, b"dummy_stdout\n");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn compile_fails() {
        let current = Path::new("./");
        let option = CompileOption {
            compiler: "/this/path/should/not/exist".into(),
            args: vec![],
        };
        assert!(compile_at(current, &option).is_err());
    }
}
