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
