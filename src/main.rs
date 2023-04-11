use std::{fs, path::Path};

use clap::Parser;
use evalsa_worker::{compile_at, launch, CompileOption, LaunchOption};
use evalsa_worker_proto::{RouterBound, Run, RunResult};
use tempfile::tempdir;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    host: String,
}

fn main() {
    let args = Args::parse();
    let rustc = Path::new("rustc");
    let nsjail = Path::new("nsjail");

    let ctx = zmq::Context::new();

    let socket = ctx.socket(zmq::DEALER).unwrap();
    socket.connect(&args.host).unwrap();

    let capability = bincode::serialize(&RouterBound::Capability {
        languages: vec!["rust".into()],
    })
    .unwrap();
    socket.send(capability, 0).unwrap();

    loop {
        let idle = bincode::serialize(&RouterBound::Idle).unwrap();
        socket.send(idle, 0).unwrap();
        let enqueue: Run = bincode::deserialize(&socket.recv_bytes(0).unwrap()).unwrap();
        if enqueue.language != "rust" {
            let reject = bincode::serialize(&RouterBound::Reject).unwrap();
            socket.send(reject, 0).unwrap();
            continue;
        }
        {
            let temp = tempdir().unwrap();
            let src = temp.path().join("main.rs");
            fs::write(&src, enqueue.code).unwrap();
            compile_at(
                temp.path(),
                &CompileOption {
                    compiler: rustc.to_path_buf(),
                    args: vec!["-O".into(), src.as_os_str().into()],
                },
            )
            .unwrap();
            let bin = temp.path().join("main");
            let output = launch(
                nsjail,
                &src,
                &LaunchOption {
                    binary: bin,
                    args: vec![],
                    stdin: vec![],
                    time: 1,
                    virtual_memory: 16,
                    files_size: 0,
                    proc_count: 1,
                    mount_proc: false,
                    seccomp: None,
                },
            )
            .unwrap();
            if output.status.success() {
                let finished = bincode::serialize(&RouterBound::Finished {
                    result: RunResult::Success,
                    exit_code: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                })
                .unwrap();
                socket.send(finished, 0).unwrap();
            } else {
                let finished = bincode::serialize(&RouterBound::Finished {
                    result: RunResult::RuntimeError,
                    exit_code: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                })
                .unwrap();
                socket.send(finished, 0).unwrap();
            }
        }
    }
}
