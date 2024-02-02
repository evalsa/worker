use amiquip::{
    Connection, ConsumerMessage, ConsumerOptions, Exchange, Publish, QueueDeclareOptions,
};
use evalsa_worker::{launch, Language, LaunchOption, LaunchStatus, Sandbox};
use evalsa_worker_proto::{Finished, Run, RunResult, Running, RunningState};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Config {
    languages: Vec<Language>,
    sandbox: Sandbox,
}

fn main() {
    let config_file = std::fs::read_to_string("config.toml").unwrap();
    let config: Config = toml::from_str(&config_file).unwrap();
    let mut connection = Connection::insecure_open("amqp://localhost:5672").unwrap();
    let channel = connection.open_channel(None).unwrap();
    channel
        .queue_declare("apibound", QueueDeclareOptions::default())
        .unwrap();
    let workerbound = channel
        .queue_declare("workerbound", QueueDeclareOptions::default())
        .unwrap();
    let apibound = Exchange::direct(&channel);
    let consumer = workerbound.consume(ConsumerOptions::default()).unwrap();

    for message in consumer.receiver().iter() {
        match message {
            ConsumerMessage::Delivery(delivery) => {
                let run: Run = ciborium::from_reader(delivery.body.as_slice()).unwrap();
                if let Some(language) = config.languages.iter().find(|&l| l.name == run.language) {
                    delivery.ack(&channel).unwrap();
                    let mut body = vec![];
                    ciborium::into_writer(
                        &Running {
                            id: run.id,
                            state: RunningState::Fetched,
                        },
                        &mut body,
                    )
                    .unwrap();
                    apibound.publish(Publish::new(&body, "apibound")).unwrap();
                    let result = launch(
                        &run.code,
                        &run.stdin,
                        language,
                        &config.sandbox,
                        &LaunchOption {
                            timeout: 1000,
                            max_virtual_memory: 1 << 30,
                        },
                    );
                    let mut body = vec![];
                    let run_result = match result.status {
                        LaunchStatus::Exit(code) => RunResult::Exit {
                            exit_code: code,
                            cpu_time: result.user_time_ms as i32,
                            memory: result.memory_kib as i32,
                        },
                        LaunchStatus::CompilationError => RunResult::CompilationError,
                        LaunchStatus::RuntimeError => RunResult::RuntimeError,
                        LaunchStatus::OutputLimitExceeded => RunResult::OutputLimitExceeded,
                        LaunchStatus::TimeLimitExceeded => RunResult::TimeLimitExceeded,
                    };
                    ciborium::into_writer(
                        &Running {
                            id: run.id,
                            state: RunningState::Finished(Finished {
                                result: run_result,
                                stdout: result.stdout,
                                stderr: result.stderr,
                            }),
                        },
                        &mut body,
                    )
                    .unwrap();
                    apibound.publish(Publish::new(&body, "apibound")).unwrap();
                }
            }
            _ => break,
        }
    }
}
