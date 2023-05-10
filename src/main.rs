use core::fmt;
use std::io::{stdin, stdout, BufRead, Read, Write};
use std::time::Instant;

use base64::engine::general_purpose;
use base64::Engine;
use clap::{Parser, Subcommand};
use reqwest::Url;
use serde_derive::Deserialize;
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};
use toml::Value;

#[derive(Parser, Debug)]
#[clap(about = "A CLI client for interacting with Faasten")]
struct Cli {
    #[clap(subcommand)]
    command: Action,
    #[clap(short, long, value_parser)]
    server: Option<String>,
}

#[derive(Parser, Debug)]
struct Invoke {
    function: String,
    payload: Option<String>,
}

#[derive(Parser, Debug)]
struct Register {
    /// Path to the local image
    local_path: String,
    /// Ignored if not logged in
    policy: String,
    /// Path to the gate
    remote_path: String,
    /// VM memory size
    mem_in_mb: usize,
    /// runtime: python
    runtime: String,
}

#[derive(Parser, Debug)]
struct ListDir {
    /// Path to the directory
    path: String,
}

#[derive(Parser, Debug)]
struct ListFaceted {
    /// Path to the directory
    path: String,
}

#[derive(Parser, Debug)]
struct CreateFile {
    /// Path to the directory
    path: String,
    label: String,
}

#[derive(Parser, Debug)]
struct WriteFile {
    /// Path to the directory
    path: String,
}

#[derive(Parser, Debug)]
struct ReadFile {
    /// Path to the directory
    path: String,
}

#[derive(Parser, Debug)]
struct Ping {}

#[derive(Parser, Debug)]
struct PingScheduler {}

#[derive(Subcommand, Debug)]
enum Action {
    /// Login to Faasten
    Login,
    /// Invoke a gate
    Invoke(Invoke),
    /// upload local image to a faasten
    Register(Register),
    /// ping gateway
    Ping(Ping),
    /// ping scheduler via gateway
    PingScheduler(PingScheduler),
    /// list a directory
    ListDir(ListDir),
    /// list a faceted directory
    ListFaceted(ListFaceted),
    /// create a file
    CreateFile(CreateFile),
    /// write from stdin to a file
    WriteFile(WriteFile),
    /// read a file to stdout as raw bytes
    ReadFile(ReadFile),
}

fn status(
    stream: &mut StandardStream,
    action: &dyn fmt::Display,
    status: &dyn fmt::Display,
) -> Result<(), std::io::Error> {
    stream.set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Green)))?;
    write!(stream, "{:>12} ", action)?;
    stream.reset()?;
    writeln!(stream, "{}", status)
}

fn check_credential(server: &String) -> Result<String, std::io::Error> {
    let config_dir = dirs::config_dir()
        .unwrap_or("~/.config".into())
        .join("fstn");
    std::fs::create_dir_all(&config_dir)?;
    let credentials_file = config_dir.join("credentials");
    let creds: Value = toml::from_slice(&std::fs::read(credentials_file)?)?;
    if let Some(token) = creds
        .get(server)
        .and_then(|v| v.get("token"))
        .and_then(Value::as_str)
    {
        Ok(String::from(token))
    } else if let Some(token) = creds.get("token").and_then(Value::as_str) {
        Ok(String::from(token))
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "no token found",
        ))
    }
}

const DEFAULT_SERVER: &'static str = "https://snapfaas.princeton.systems";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let server = cli
        .server
        .or(std::env::var("FSTN_SERVER").ok())
        .unwrap_or(String::from(DEFAULT_SERVER));
    let mut stderr = StandardStream::stderr(termcolor::ColorChoice::Auto);
    let client = reqwest::blocking::Client::new();
    match cli.command {
        Action::Login => {
            println!(
                "Please paste the API Token found by logging in at {}/login/cas below",
                server
            );
            if let Some(Ok(token)) = stdin().lock().lines().next() {
                let config_dir = dirs::config_dir()
                    .unwrap_or("~/.config".into())
                    .join("fstn");
                std::fs::create_dir_all(&config_dir)?;
                let credentials_file = config_dir.join("credentials");
                let mut credentials: Value = if credentials_file.exists() {
                    toml::from_slice(&std::fs::read(&credentials_file)?)?
                } else {
                    Value::Table(Default::default())
                };
                credentials.as_table_mut().and_then(|t| {
                    t.insert(
                        server,
                        Value::Table(toml::map::Map::from_iter([(
                            String::from("token"),
                            Value::String(token),
                        )])),
                    )
                });
                std::fs::write(credentials_file, toml::to_string(&credentials)?)?;
                status(&mut stderr, &"Login", &"saved")?;
            }
        }
        Action::Invoke(Invoke { function, payload }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/faasten/invoke/{}", server, function).as_str())?;
                let payload = if let Some(p) = payload {
                    p
                } else {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    buf
                };
                let mut result = client
                    .post(url)
                    .bearer_auth(&token)
                    .header("content-type", "application/json")
                    .body(payload)
                    .send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut stdout())?;
                    status(&mut stderr, &"Invoke", &"OK")?;
                } else {
                    status(&mut stderr, &"Invoke", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            } else {
                status(&mut stderr, &"Invoke", &"you must first login")?;
            }
        }
        Action::Ping(Ping {}) => {
            let now = Instant::now();
            let url = Url::parse(format!("{}/faasten/ping", server).as_str())?;
            let _ = client.get(url).send()?;
            println!("ping: {:?} elapsed", now.elapsed());
        }
        Action::PingScheduler(PingScheduler {}) => {
            let now = Instant::now();
            let url = Url::parse(format!("{}/faasten/ping/scheduler", server).as_str())?;
            let _ = client.get(url).send()?;
            println!("ping: {:?} elapsed", now.elapsed());
        }
        Action::Register(Register {
            local_path,
            policy,
            remote_path,
            mem_in_mb,
            runtime,
        }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/faasten/invoke/~:fsutil", server).as_str())?;
                let form = reqwest::blocking::multipart::Form::new()
                    .text(
                        "payload",
                        serde_json::json!({
                            "op": "create-gate",
                            "args": {
                                "path": remote_path,
                                "policy": policy,
                                "memory": mem_in_mb,
                                "runtime": runtime
                            }
                        })
                        .to_string(),
                    )
                    // the actual label is lub(label, lub({labels of path components}))
                    // this request is constrained by a clearance = login,login.
                    .text("label", "T,T")
                    .file("file", local_path)?;
                let mut result = client
                    .post(url)
                    .bearer_auth(&token)
                    .multipart(form)
                    .send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut stdout())?;
                    status(&mut stderr, &"Register", &"OK")?;
                } else {
                    status(&mut stderr, &"Register", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            } else {
                status(&mut stderr, &"Register", &"you must first log in")?;
            }
        }
        Action::ListDir(ListDir { path }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/faasten/invoke/~:fsutil", server).as_str())?;
                let mut result = client
                    .post(url)
                    .bearer_auth(&token)
                    .header("content-type", "application/json")
                    .body(
                        serde_json::json!({
                            "op": "list-dir",
                            "args": {
                                "path": path,
                            }
                        })
                        .to_string(),
                    )
                    .send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut stdout())?;
                    status(&mut stderr, &"ListDir", &"OK")?;
                } else {
                    status(&mut stderr, &"ListDir", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            } else {
                status(&mut stderr, &"ListDir", &"you must first log in.")?;
            }
        }
        Action::ListFaceted(ListFaceted { path }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/faasten/invoke/~:fsutil", server).as_str())?;
                let mut result = client
                    .post(url)
                    .bearer_auth(&token)
                    .header("content-type", "application/json")
                    .body(
                        serde_json::json!({
                            "op": "list-faceted",
                            "args": {
                                "path": path,
                            }
                        })
                        .to_string(),
                    )
                    .send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut stdout())?;
                    status(&mut stderr, &"ListFaceted", &"OK")?;
                } else {
                    status(&mut stderr, &"ListFaceted", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            } else {
                status(&mut stderr, &"ListFaceted", &"you must first log in.")?;
            }
        }
        Action::CreateFile(CreateFile { path, label }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/faasten/invoke/~:fsutil", server).as_str())?;
                let mut result = client
                    .post(url)
                    .bearer_auth(&token)
                    .header("content-type", "application/json")
                    .body(
                        serde_json::json!({
                            "op": "create-file",
                            "args": {
                                "path": path,
                                "label": label,
                            }
                        })
                        .to_string(),
                    )
                    .send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut stdout())?;
                    status(&mut stderr, &"CreateFile", &"OK")?;
                } else {
                    status(&mut stderr, &"CreateFile", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            } else {
                status(&mut stderr, &"CreateFile", &"you must first log in.")?;
            }
        }
        Action::WriteFile(WriteFile { path }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/faasten/invoke/~:fsutil", server).as_str())?;
                let mut stdin = std::io::stdin();
                let buf = &mut vec![];
                stdin.read_to_end(buf).expect("read stdin");

                let encoded = general_purpose::STANDARD.encode(buf);

                let mut result = client
                    .post(url)
                    .bearer_auth(&token)
                    .header("content-type", "application/json")
                    .body(
                        serde_json::json!({
                            "op": "write-file",
                            "args": {
                                "path": path,
                                "data": encoded,
                            }
                        })
                        .to_string(),
                    )
                    .send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut stdout())?;
                    status(&mut stderr, &"WriteFile", &"OK")?;
                } else {
                    status(&mut stderr, &"WriteFile", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            } else {
                status(&mut stderr, &"WriteFile", &"you must first log in.")?;
            }
        }
        Action::ReadFile(ReadFile { path }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/faasten/invoke/~:fsutil", server).as_str())?;

                let mut result = client
                    .post(url)
                    .bearer_auth(&token)
                    .header("content-type", "application/json")
                    .body(
                        serde_json::json!({
                            "op": "read-file",
                            "args": {
                                "path": path,
                            }
                        })
                        .to_string(),
                    )
                    .send()?;
                if result.status().is_success() {
                    #[derive(Deserialize)]
                    struct ReadRet {
                        #[allow(dead_code)]
                        success: bool,
                        value: String,
                    }
                    let ret: ReadRet = result.json().unwrap();
                    let mut decoded = general_purpose::STANDARD.decode(ret.value).unwrap();
                    stdout().write_all(decoded.as_mut())?;
                    status(&mut stderr, &"ReadFile", &"OK")?;
                } else {
                    status(&mut stderr, &"ReadFile", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            } else {
                status(&mut stderr, &"ReadFile", &"you must first log in.")?;
            }
        }
    }
    Ok(())
}
