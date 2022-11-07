use std::{ffi::OsString, path::Path, process::Command};

/// Options provided for compiling binaries.
struct CompileOption {
    /// File to execute when compile.
    pub compiler: OsString,
    /// Arguments passed to compiler process.
    pub args: Vec<OsString>,
}

/// Result of compilation.
struct CompileResult {
    /// If compiler exits normally then `true`, else `false`.
    pub success: bool,
    /// Message from standard error stream of compiler process.
    pub message: Vec<u8>,
}

/// Compiles source inside directory with options.
///
/// Returns a `CompileResult` object containing success flag and standard error stream data.
fn compile_at(directory: &Path, option: &CompileOption) -> CompileResult {
    Command::new(&option.compiler)
        .current_dir(directory)
        .args(option.args.iter().map(OsString::as_os_str))
        .output()
        .map(|output| CompileResult {
            success: output.status.success(),
            message: output.stderr,
        })
        .unwrap_or_else(|e| CompileResult {
            success: false,
            message: e.to_string().into_bytes(),
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
        let result = compile_at(current, &option);
        assert!(result.success);
        assert!(result.message.is_empty());
    }

    #[test]
    fn compile_return_one_fails() {
        let current = Path::new("./");
        let option = CompileOption {
            compiler: "/usr/sbin/false".into(),
            args: vec![],
        };
        let result = compile_at(current, &option);
        assert!(!result.success);
        assert!(result.message.is_empty());
    }

    #[test]
    fn compile_get_stderr() {
        let current = Path::new("./");
        let option = CompileOption {
            compiler: "/usr/bin/bash".into(),
            args: vec!["-c".into(), "echo dummy_stderr 1>&2".into()],
        };
        let result = compile_at(current, &option);
        assert!(result.success);
        assert_eq!(&result.message, b"dummy_stderr\n");
    }
}
