use std::{fs, path::Path};

use clap::Parser;
use evalsa_worker::{compile_at, launch, CompileOption, LaunchOption};
use evalsa_worker_proto::{ApiBound, Finished, Run, RunResult};
use tempfile::tempdir;
use zmq::Message;

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

    let socket = ctx.socket(zmq::REQ).unwrap();
    socket.connect(&args.host).unwrap();

    let languages = vec!["rust".into()];
    let mut msg = Message::new();
    loop {
        let idle = bincode::serialize(&ApiBound::Idle {
            languages: languages.clone(),
        })
        .unwrap();
        socket.send(idle, 0).unwrap();
        socket.recv(&mut msg, 0).unwrap();
        let enqueue: Run = bincode::deserialize(&msg).unwrap();
        if !languages.contains(&enqueue.language) {
            let serialized = bincode::serialize(&ApiBound::Reject { id: enqueue.id }).unwrap();
            socket.send(&serialized, 0).unwrap();
            socket.recv(&mut msg, 0).unwrap();
            continue;
        }
        {
            let serialized = bincode::serialize(&ApiBound::Fetched).unwrap();
            socket.send(&serialized, 0).unwrap();
            socket.recv(&mut msg, 0).unwrap();
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
                bincode::serialize_into(
                    msg.as_mut(),
                    &ApiBound::Finished(Finished {
                        id: enqueue.id,
                        result: RunResult::Success,
                        exit_code: output.status.code(),
                        stdout: output.stdout,
                        stderr: output.stderr,
                    }),
                )
                .unwrap();
                socket.send(msg.as_ref(), 0).unwrap();
            } else {
                let serialized = bincode::serialize(&ApiBound::Finished(Finished {
                    id: enqueue.id,
                    result: RunResult::RuntimeError,
                    exit_code: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                }))
                .unwrap();
                socket.send(&serialized, 0).unwrap();
                socket.recv(&mut msg, 0).unwrap();
            }
        }
    }
}
