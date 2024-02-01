use std::{
    ffi::CString,
    time::{Duration, Instant},
};

use libc::wait4;
use nix::{
    errno::errno,
    fcntl::OFlag,
    mount::MsFlags,
    sched::{clone, CloneFlags},
    sys::{
        resource::{setrlimit, Resource},
        signal::{kill, Signal},
        stat::Mode,
    },
    unistd::{chdir, chroot, dup2},
};
use serde::Deserialize;
use tempfile::tempdir;

#[derive(Deserialize, Debug)]
pub struct Language {
    pub name: String,
    pub source: String,
    pub compile: Option<String>,
    pub execute: String,
    pub args: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct Sandbox {
    pub stack_size_bytes: usize,
    pub mounts: Vec<Mount>,
}

#[derive(Deserialize, Debug)]
pub struct Mount {
    pub source: String,
    pub destination: String,
}

#[derive(Deserialize, Debug)]
pub struct LaunchOption {
    pub timeout: u64,
    pub max_virtual_memory: u64,
}

#[derive(Deserialize, Debug)]
pub struct LaunchResult {
    pub status: LaunchStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub memory_kib: i64,
    pub user_time_ms: i64,
}

#[derive(Deserialize, Debug)]
pub enum LaunchStatus {
    Exit(i32),
    CompilationError,
    RuntimeError,
    OutputLimitExceeded,
    TimeLimitExceeded,
}

pub fn launch(
    code: &[u8],
    stdin: &[u8],
    language: &Language,
    sandbox: &Sandbox,
    option: &LaunchOption,
) -> LaunchResult {
    let dir = tempdir().unwrap();
    let path = dir.path();
    let source_path = path.join(&language.source);
    std::fs::write(source_path, code).unwrap();
    if let Some(compile) = &language.compile {
        let (stderr_rx, stderr_tx) = nix::unistd::pipe().unwrap();
        let execute = CString::new("/bin/bash").unwrap();
        let execute_args = vec![
            execute.clone(),
            CString::new("-c").unwrap(),
            CString::new(compile.as_str()).unwrap(),
        ];
        let mut stack = vec![0; 1048576];
        let pid = unsafe {
            let path = path.as_os_str().to_owned();
            clone(
                Box::new(move || {
                    chdir(path.as_os_str()).unwrap();
                    nix::unistd::close(2).unwrap();
                    dup2(stderr_tx, 2).unwrap();
                    nix::unistd::close(stderr_tx).ok();
                    nix::unistd::execv(&execute, &execute_args).unwrap();
                    0
                }),
                &mut stack,
                CloneFlags::CLONE_NEWNET | CloneFlags::CLONE_NEWUSER,
                Some(nix::libc::SIGCHLD),
            )
        }
        .unwrap();
        let deadline = Instant::now() + Duration::from_millis(option.timeout);
        let mut usage = empty_rusage();
        let mut wait;
        let mut wait_status = 0;
        loop {
            wait = unsafe { wait4(pid.as_raw(), &mut wait_status, libc::WNOHANG, &mut usage) };
            if wait != 0 {
                break;
            }
            if Instant::now() > deadline {
                kill(pid, Signal::SIGKILL).unwrap();
            }
        }
        if wait < 0 {
            panic!("{:?}", std::io::Error::from_raw_os_error(errno()));
        }
        nix::unistd::close(stderr_tx).unwrap();
        if !libc::WIFEXITED(wait_status) || libc::WEXITSTATUS(wait_status) != 0 {
            let mut buffer = vec![0; 8192];
            let mut stderr = vec![];
            loop {
                let len = nix::unistd::read(stderr_rx, &mut buffer).unwrap();
                if len == 0 {
                    break;
                }
                stderr.extend_from_slice(&buffer[..len]);
                if stderr.len() > 2 << 10 {
                    break;
                }
            }
            return LaunchResult {
                status: LaunchStatus::CompilationError,
                stdout: vec![],
                stderr,
                memory_kib: 0,
                user_time_ms: 0,
            };
        }
    }
    let stdin_path = path.join("stdin");
    std::fs::write(stdin_path, stdin).unwrap();
    let (stdout_rx, stdout_tx) = nix::unistd::pipe().unwrap();
    let (stderr_rx, stderr_tx) = nix::unistd::pipe().unwrap();
    let mut stack = vec![0; sandbox.stack_size_bytes];
    let execute = CString::new(language.execute.clone()).unwrap();
    let mut execute_args = vec![execute.clone()];
    execute_args.extend(
        language
            .args
            .iter()
            .map(|arg| CString::new(arg.clone()).unwrap()),
    );
    for mount in &sandbox.mounts {
        let destination = path.join(mount.destination.trim_start_matches('/'));
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(destination.as_path())
            .unwrap();
        nix::mount::mount(
            Some(mount.source.as_str()),
            destination.as_path(),
            Option::<&str>::None,
            MsFlags::MS_BIND | MsFlags::MS_RDONLY | MsFlags::MS_NOATIME | MsFlags::MS_NODIRATIME,
            Option::<&str>::None,
        )
        .unwrap();
    }
    let pid = unsafe {
        let path = path.as_os_str().to_owned();
        clone(
            Box::new(move || {
                chdir(path.as_os_str()).unwrap();
                chroot(path.as_os_str()).unwrap();
                nix::unistd::close(0).unwrap();
                nix::fcntl::open("/stdin", OFlag::O_RDONLY, Mode::empty()).unwrap();
                nix::unistd::close(1).unwrap();
                dup2(stdout_tx, 1).unwrap();
                nix::unistd::close(stdout_tx).unwrap();
                nix::unistd::close(2).unwrap();
                dup2(stderr_tx, 2).unwrap();
                nix::unistd::close(stderr_tx).ok();
                setrlimit(Resource::RLIMIT_CORE, 0, 0).unwrap();
                setrlimit(Resource::RLIMIT_FSIZE, 0, 0).unwrap();
                setrlimit(
                    Resource::RLIMIT_AS,
                    option.max_virtual_memory,
                    option.max_virtual_memory,
                )
                .unwrap();
                nix::unistd::execve::<_, CString>(&execute, &execute_args, &[]).unwrap();
                0
            }),
            &mut stack,
            CloneFlags::CLONE_NEWNET | CloneFlags::CLONE_NEWUSER,
            Some(nix::libc::SIGCHLD),
        )
    }
    .unwrap();
    let deadline = Instant::now() + Duration::from_millis(option.timeout);
    let mut usage = empty_rusage();
    let mut wait;
    let mut wait_status = 0;
    loop {
        wait = unsafe { wait4(pid.as_raw(), &mut wait_status, libc::WNOHANG, &mut usage) };
        if wait != 0 {
            break;
        }
        if Instant::now() > deadline {
            kill(pid, Signal::SIGKILL).unwrap();
        }
    }
    for mount in &sandbox.mounts {
        let destination = path.join(mount.destination.trim_start_matches('/'));
        nix::mount::umount(destination.as_path()).unwrap();
    }
    if wait < 0 {
        panic!("{:?}", std::io::Error::from_raw_os_error(errno()));
    }
    let mut status = if libc::WIFEXITED(wait_status) {
        LaunchStatus::Exit(libc::WEXITSTATUS(wait_status))
    } else if libc::WTERMSIG(wait_status) == libc::SIGKILL {
        LaunchStatus::TimeLimitExceeded
    } else {
        LaunchStatus::RuntimeError
    };
    nix::unistd::close(stdout_tx).unwrap();
    nix::unistd::close(stderr_tx).unwrap();
    let mut buffer = vec![0; 8192];
    let mut stdout = vec![];
    loop {
        let len = nix::unistd::read(stdout_rx, &mut buffer).unwrap();
        if len == 0 {
            break;
        }
        stdout.extend_from_slice(&buffer[..len]);
        if stdout.len() > 256 << 20 {
            status = LaunchStatus::OutputLimitExceeded;
        }
    }
    let mut stderr = vec![];
    loop {
        let len = nix::unistd::read(stderr_rx, &mut buffer).unwrap();
        if len == 0 {
            break;
        }
        stderr.extend_from_slice(&buffer[..len]);
        if stderr.len() > 2 << 10 {
            break;
        }
    }
    LaunchResult {
        status,
        stdout,
        stderr,
        memory_kib: usage.ru_majflt * 4,
        user_time_ms: usage.ru_utime.tv_sec * 1000 + usage.ru_utime.tv_usec / 1000,
    }
}

fn empty_rusage() -> libc::rusage {
    libc::rusage {
        ru_utime: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        ru_stime: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    }
}
