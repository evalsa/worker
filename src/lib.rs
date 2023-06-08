use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Options provided for compiling binaries.
pub struct CompileOption {
    /// File to execute when compile.
    pub compiler: PathBuf,
    /// Arguments passed to compiler process.
    pub args: Vec<OsString>,
}

/// Compiles source inside directory with options.
///
/// Returns a `CompileResult` object containing success flag and standard error stream data.
pub fn compile_at(directory: &Path, option: &CompileOption) -> std::io::Result<Output> {
    Command::new(&option.compiler)
        .current_dir(directory)
        .args(option.args.iter().map(OsString::as_os_str))
        .output()
}

/// Options provided for binary launch.
pub struct LaunchOption {
    /// Path to binary to launch.
    pub binary: PathBuf,
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

/// Launches a binary with given options in nsjail, change root to `directory`.
pub fn launch(nsjail: &Path, directory: &Path, option: &LaunchOption) -> std::io::Result<Output> {
    let time = option.time.to_string();
    let virtual_memory = option.virtual_memory.to_string();
    let files_size = option.files_size.to_string();
    let proc_count = option.proc_count.to_string();
    let mut command = Command::new(nsjail);
    command
        .args(["-Me"]) // use execve
        .args(["-c".as_ref(), directory.as_os_str()]) // chroot to `directory`
        .args(["-H", "worker"]) // set hostname
        .args(["-R".as_ref(), option.binary.as_path()])
        .args(["-R", "/lib"])
        .args(["-R", "/lib64"])
        .args(["-R", "/usr"])
        .arg("-Q") // Log to stderr only fatal messages
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
    command
        .arg("--")
        .arg(&option.binary)
        .args(&option.args)
        .output()
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

    #[test]
    fn nsjail_succeeds() {
        let nsjail = Path::new("nsjail");
        let directory = Path::new("./");
        let option = LaunchOption {
            binary: "/bin/bash".into(),
            args: vec!["-c".into(), "pwd".into()],
            stdin: vec![],
            time: 1,
            virtual_memory: 32,
            files_size: 0,
            proc_count: 1,
            mount_proc: false,
            seccomp: None,
        };
        let result = launch(nsjail, directory, &option).unwrap();
        assert!(result.status.success());
        assert_eq!(&result.stdout, b"/\n");
        assert!(result.stderr.is_empty());
    }
}
