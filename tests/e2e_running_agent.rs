#![cfg(all(unix, feature = "test-support"))]

//! End-to-end coverage for Phase 2 AppPlatform paths.
//!
//! This file intentionally has two layers:
//! - a fast in-process AppPlatform integration test over UDS
//! - a process-level E2E that launches the real agent binary, the payload app
//!   binary, and a local MQTT recorder

use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_core::security::{Action, Resource};
use app_sdk::{AppClient, HealthStatus};
use cc_rdeviceagent::test_support::{SpawnedAgent, spawn_app_platform_server};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};

async fn setup_agent() -> SpawnedAgent {
    spawn_app_platform_server()
        .await
        .expect("spawn AppPlatform test server")
}

#[tokio::test]
async fn app_platform_sdk_flow_over_uds_records_audit_chain() {
    let agent = setup_agent().await;
    let socket_path = agent
        .socket_path
        .to_str()
        .expect("socket path should be valid UTF-8");

    let mut client = AppClient::connect_uds(
        socket_path,
        "e2e-test-app",
        "1.0.0",
        vec!["metrics".into(), "health".into()],
    )
    .await
    .expect("connect_uds");

    let app_id = client.app_id().to_string();
    assert!(!app_id.is_empty(), "app_id must be non-empty");
    assert!(
        app_id.starts_with("e2e-test-app_"),
        "app_id should be prefixed with app name, got: {app_id}"
    );

    assert!(client.heartbeat().await.expect("heartbeat"));

    client
        .report_health(
            HealthStatus::HealthHealthy,
            "e2e test running",
            std::collections::HashMap::from([("iteration".into(), "0".into())]),
        )
        .await
        .expect("report_health");

    let message_id = client
        .publish("demo/test", b"hello-from-e2e")
        .await
        .expect("publish");
    assert!(!message_id.is_empty(), "message_id should be non-empty");

    let published = agent.publisher.drain();
    assert_eq!(published.len(), 1, "should have 1 published message");
    let (topic, payload) = &published[0];
    assert!(
        topic.contains(&app_id),
        "topic should contain app_id. topic={topic}, app_id={app_id}"
    );
    assert!(
        topic.starts_with("test-tenant/test-device-e2e/apps/"),
        "topic should follow expected pattern, got: {topic}"
    );
    assert_eq!(payload, b"hello-from-e2e");

    let config = client.get_config(vec![]).await.expect("get_config");
    assert!(config.len() < 1000, "config should be a reasonable size");

    client.unregister().await.expect("unregister_app");

    {
        let store_guard = agent.store.lock().unwrap();
        let chain = store_guard.load_audit_chain().expect("load audit chain");

        assert!(
            !chain.events().is_empty(),
            "audit chain should contain events"
        );
        assert!(chain.verify(), "audit chain must pass integrity check");

        let events = chain.events();
        assert!(
            events.iter().any(|e| {
                e.resource == Resource::AppControl
                    && e.action == Action::Execute
                    && e.result == "success"
                    && e.principal.contains("e2e-test-app")
            }),
            "audit chain should contain RegisterApp event"
        );
        assert!(
            events.iter().any(|e| {
                e.resource == Resource::Telemetry
                    && e.action == Action::Write
                    && e.result == "success"
            }),
            "audit chain should contain PublishData event"
        );
        assert!(
            events.iter().any(|e| {
                e.resource == Resource::AppControl
                    && e.action == Action::Execute
                    && e.result == "success"
                    && e.target.contains("session")
            }),
            "audit chain should contain UnregisterApp event"
        );
    }

    agent.shutdown().await;
}

#[tokio::test]
async fn running_agent_process_accepts_payload_app_and_publishes_to_mqtt_mock() {
    let temp = TempDir::new().expect("create temp dir");
    let socket_path = temp.path().join("app.sock");
    let state_db_path = temp.path().join("state.db");
    let agent_log = temp.path().join("agent.log");
    let payload_log = temp.path().join("payload.log");

    let mqtt = MqttRecorder::start().await.expect("start mqtt recorder");
    let config_path =
        write_agent_config(temp.path(), &socket_path, mqtt.addr()).expect("write e2e agent config");

    let mut agent = spawn_agent_process(&config_path, &state_db_path, &agent_log)
        .expect("spawn cc-rdeviceagent");
    wait_for_path(&socket_path, Duration::from_secs(10))
        .await
        .unwrap_or_else(|error| panic_with_logs(error, &agent_log, &payload_log));

    let mut payload = spawn_payload_process(&socket_path, &payload_log).expect("spawn payload app");
    let payload_status = tokio::time::timeout(Duration::from_secs(15), payload.wait())
        .await
        .unwrap_or_else(|_| panic_with_logs("payload app timed out", &agent_log, &payload_log))
        .unwrap_or_else(|error| panic_with_logs(error, &agent_log, &payload_log));
    assert!(
        payload_status.success(),
        "payload app failed with {payload_status}. payload log:\n{}",
        read_log(&payload_log)
    );

    let publish = mqtt
        .wait_for_publish(Duration::from_secs(10))
        .await
        .unwrap_or_else(|error| panic_with_logs(error, &agent_log, &payload_log));
    assert!(
        publish
            .topic
            .starts_with("default/e2e-device/apps/payload-hello_"),
        "unexpected MQTT app topic: {}",
        publish.topic
    );
    assert!(
        publish.topic.ends_with("/demo/hello"),
        "unexpected MQTT app topic: {}",
        publish.topic
    );
    assert!(
        String::from_utf8_lossy(&publish.payload).starts_with("hello-from-payload-"),
        "unexpected MQTT app payload: {:?}",
        publish.payload
    );

    terminate_child(&mut agent).await;

    let store = agent_store::StateStore::open(&state_db_path).expect("open e2e state db");
    let chain = store.load_audit_chain().expect("load e2e audit chain");
    assert!(chain.verify(), "process E2E audit chain must verify");
    assert!(
        chain.events().iter().any(|event| {
            event.resource == Resource::Telemetry
                && event.action == Action::Write
                && event.target.contains("payload-hello_")
                && event.result == "success"
        }),
        "audit chain should contain payload PublishData success event"
    );
}

fn write_agent_config(
    dir: &Path,
    socket_path: &Path,
    mqtt_addr: SocketAddr,
) -> io::Result<PathBuf> {
    let config_path = dir.join("CC-rDeviceAgent.toml");
    fs::write(
        &config_path,
        format!(
            r#"[service]
service_name = "CC-rDeviceAgent"
device_id = "e2e-device"
state_interval_seconds = 1
watched_processes = []
udp_display_target = "127.0.0.1:9008"
launcher_proxy_path = ""

[control]
listen_addr = "127.0.0.1:0"

[app_platform]
enabled = true
socket_path = "{}"
session_duration_secs = 60

[mqtt]
enabled = true
broker_host = "{}"
broker_port = {}
telemetry_enabled = false
status_enabled = false
"#,
            socket_path.display(),
            mqtt_addr.ip(),
            mqtt_addr.port()
        ),
    )?;
    Ok(config_path)
}

fn spawn_agent_process(
    config_path: &Path,
    state_db_path: &Path,
    log_path: &Path,
) -> io::Result<Child> {
    let log = fs::File::create(log_path)?;
    Command::new(env!("CARGO_BIN_EXE_cc-rdeviceagent"))
        .arg("foreground")
        .arg("--config")
        .arg(config_path)
        .env("CC_AGENT_STATE_DB", state_db_path)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()
}

fn spawn_payload_process(socket_path: &Path, log_path: &Path) -> io::Result<Child> {
    let log = fs::File::create(log_path)?;
    Command::new(payload_hello_bin())
        .env("CC_APP_SOCKET_PATH", socket_path)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()
}

fn payload_hello_bin() -> PathBuf {
    option_env!("CARGO_BIN_EXE_payload-hello")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("target");
            path.push("debug");
            path.push("payload-hello");
            path
        })
}

async fn wait_for_path(path: &Path, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if path.exists() {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(format!("timed out waiting for {}", path.display()));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn terminate_child(child: &mut Child) {
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }

    if tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

fn panic_with_logs(error: impl std::fmt::Display, agent_log: &Path, payload_log: &Path) -> ! {
    panic!(
        "{error}\n\nagent log:\n{}\n\npayload log:\n{}",
        read_log(agent_log),
        read_log(payload_log)
    )
}

fn read_log(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| format!("<failed to read log: {error}>"))
}

#[derive(Clone, Debug)]
struct RecordedPublish {
    topic: String,
    payload: Vec<u8>,
}

struct MqttRecorder {
    addr: SocketAddr,
    published: Arc<Mutex<Vec<RecordedPublish>>>,
}

impl MqttRecorder {
    async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let published = Arc::new(Mutex::new(Vec::new()));
        let published_for_task = Arc::clone(&published);

        tokio::spawn(async move {
            while let Ok((stream, _peer)) = listener.accept().await {
                let published = Arc::clone(&published_for_task);
                tokio::spawn(async move {
                    let _ = handle_mqtt_connection(stream, published).await;
                });
            }
        });

        Ok(Self { addr, published })
    }

    fn addr(&self) -> SocketAddr {
        self.addr
    }

    async fn wait_for_publish(&self, timeout: Duration) -> Result<RecordedPublish, String> {
        let start = Instant::now();
        loop {
            if let Some(publish) = self.published.lock().unwrap().first().cloned() {
                return Ok(publish);
            }
            if start.elapsed() > timeout {
                return Err("timed out waiting for MQTT publish".to_string());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

async fn handle_mqtt_connection(
    mut stream: TcpStream,
    published: Arc<Mutex<Vec<RecordedPublish>>>,
) -> io::Result<()> {
    loop {
        let mut first = [0u8; 1];
        if stream.read_exact(&mut first).await.is_err() {
            return Ok(());
        }

        let remaining = read_remaining_length(&mut stream).await?;
        let mut body = vec![0u8; remaining];
        stream.read_exact(&mut body).await?;

        match first[0] >> 4 {
            1 => {
                stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await?;
            }
            3 => {
                if let Some((publish, packet_id)) = parse_publish(first[0], &body) {
                    published.lock().unwrap().push(publish);
                    if let Some(packet_id) = packet_id {
                        stream
                            .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
                            .await?;
                    }
                }
            }
            12 => {
                stream.write_all(&[0xd0, 0x00]).await?;
            }
            14 => return Ok(()),
            _ => {}
        }
    }
}

async fn read_remaining_length(stream: &mut TcpStream) -> io::Result<usize> {
    let mut multiplier = 1usize;
    let mut value = 0usize;
    for _ in 0..4 {
        let mut encoded = [0u8; 1];
        stream.read_exact(&mut encoded).await?;
        value += ((encoded[0] & 127) as usize) * multiplier;
        if encoded[0] & 128 == 0 {
            return Ok(value);
        }
        multiplier *= 128;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "malformed MQTT remaining length",
    ))
}

fn parse_publish(header: u8, body: &[u8]) -> Option<(RecordedPublish, Option<u16>)> {
    if body.len() < 2 {
        return None;
    }
    let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
    if body.len() < 2 + topic_len {
        return None;
    }
    let topic = String::from_utf8_lossy(&body[2..2 + topic_len]).to_string();
    let qos = (header & 0b0000_0110) >> 1;
    let mut payload_start = 2 + topic_len;
    let packet_id = if qos > 0 {
        if body.len() < payload_start + 2 {
            return None;
        }
        let id = u16::from_be_bytes([body[payload_start], body[payload_start + 1]]);
        payload_start += 2;
        Some(id)
    } else {
        None
    };
    Some((
        RecordedPublish {
            topic,
            payload: body[payload_start..].to_vec(),
        },
        packet_id,
    ))
}
