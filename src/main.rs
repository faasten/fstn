use core::fmt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use std:: io::{stdin, stdout, BufRead, Read, Write};

use backhand::NodeHeader;
use clap::{Parser, Subcommand};
use reqwest::Url;
use reqwest::blocking::Response;
use serde_with::serde_as;
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};
use toml::Value;
use serde_repr::*;
use serde_derive::{Deserialize, Serialize};
use serde_with::base64::Base64;

#[derive(Parser, Debug)]
#[clap(about = "A CLI client for interacting with Faasten")]
struct Cli {
    #[clap(subcommand)]
    command: Action,
    #[clap(short, long, value_parser)]
    server: Option<String>,
    #[clap(short, long, value_parser)]
    user: Option<String>,
}

#[derive(Parser, Debug)]
struct Invoke {
    function: String,
    payload: Option<String>,
}


#[derive(Parser, Debug)]
struct Delegate {
    #[clap(value_parser)]
    privilege: String,
    #[clap(short, long, value_parser)]
    save: bool,
    #[clap(short, long, value_parser)]
    bootstrap: bool,
    #[clap(short, long, value_parser)]
    #[arg(requires="bootstrap")]
    clearance: Option<String>,
}

#[derive(Parser, Debug)]
struct OneArg {
    #[clap(value_parser)]
    arg: String
}

#[derive(Parser, Debug)]
struct TwoArgs {
    #[clap(value_parser)]
    base: String,
    #[clap(value_parser)]
    name: String,
}

#[derive(Parser, Debug)]
struct TwoArgsLabel {
    #[clap(short, long, value_parser)]
    label: Option<String>,
    #[clap(value_parser)]
    base: String,
    #[clap(value_parser)]
    name: String,
}

#[derive(Parser, Debug)]
struct MkBlobArgs {
    #[clap(short, long, value_parser)]
    label: Option<String>,
    #[clap(value_parser)]
    base: String,
    #[clap(value_parser)]
    files: Vec<String>,
}

#[derive(Parser, Debug)]
struct MkGateArgs {
    #[clap(short, long, value_parser)]
    label: String,
    #[clap(short, long, value_parser)]
    privilege: String,
    #[clap(short, long, value_parser)]
    clearance: String,
    #[clap(short, long, value_parser)]
    #[arg(requires="app_image")]
    #[arg(requires="kernel")]
    #[arg(requires="runtime")]
    #[arg(conflicts_with="gate")]
    memory: Option<u64>,
    #[clap(short, long, value_parser)]
    app_image: Option<String>,
    #[clap(short, long, value_parser)]
    kernel: Option<String>,
    #[clap(short, long, value_parser)]
    runtime: Option<String>,
    #[clap(short, long, value_parser)]
    #[arg(conflicts_with="memory")]
    #[arg(conflicts_with="app_image")]
    #[arg(conflicts_with="kernel")]
    #[arg(conflicts_with="runtime")]
    gate: Option<String>,
    base: String,
    name: String,
}

#[derive(Parser, Debug)]
struct UpgateArgs {
    #[clap(short, long, value_parser)]
    privilege: Option<String>,
    #[clap(short, long, value_parser)]
    clearance: Option<String>,
    #[clap(short, long, value_parser)]
    #[arg(conflicts_with="gate")]
    memory: Option<u64>,
    #[clap(short, long, value_parser)]
    app_image: Option<String>,
    #[clap(short, long, value_parser)]
    kernel: Option<String>,
    #[clap(short, long, value_parser)]
    runtime: Option<String>,
    #[clap(short, long, value_parser)]
    #[arg(conflicts_with="memory")]
    #[arg(conflicts_with="app_image")]
    #[arg(conflicts_with="kernel")]
    #[arg(conflicts_with="runtime")]
    gate: Option<String>,
    path: String,
}

#[derive(Parser, Debug)]
struct MkGateArgsDirect {
    #[clap(short, long, value_parser)]
    memory: Option<u64>,
    #[clap(short, long, value_parser)]
    app_image: Option<String>,
    #[clap(short, long, value_parser)]
    kernel: Option<String>,
    #[clap(short, long, value_parser)]
    runtime: Option<String>,
}

#[derive(Parser, Debug)]
struct InvokeArgs {
    #[clap(value_parser)]
    path: String,
    #[clap(value_parser = param_valid)]
    params: Vec<(String, String)>,
}

fn param_valid(s: &str) -> Result<(String, String), String> {
    let (k, v) = s.split_once("=").ok_or("argument must be of the form key=value".to_string())?;
    Ok((k.to_string(), v.to_string()))
}

#[derive(Subcommand, Debug)]
enum FsOp {
    Ping,
    Ls(OneArg),
    Unlink(TwoArgs),
    Mkdir(TwoArgsLabel),
    Mkfile(TwoArgsLabel),
    Write(OneArg),
    Read(OneArg),
    Mkgate(MkGateArgs),
    Upgate(UpgateArgs),
    Mkblob(MkBlobArgs),
    Cat(OneArg),
    Mkfaceted(TwoArgs),
    Mksvc(TwoArgsLabel),
    Invoke(InvokeArgs),
}

#[derive(Parser, Debug)]
struct FS {
    #[clap(subcommand)]
    op: FsOp,
    #[clap(short, long, value_parser)]
    masquerade: Option<String>,
}

#[derive(Parser, Debug)]
struct Ping {}

#[derive(Parser, Debug)]
struct PingScheduler {}

#[derive(Parser, Debug)]
struct Build {
    source_dir: PathBuf,
    #[clap(short, long, value_parser)]
    output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Action {
    /// Login to Faasten
    Login,
    // Who am I?
    Whoami,
    /// Delegate a privilege
    Delegate(Delegate),
    /// Invoke a gate
    Invoke(Invoke),
    /// upload local image to a faasten
    /// File system operaions
    FS(FS),
    /// ping gateway
    Ping(Ping),
    /// ping scheduler via gateway
    PingScheduler(PingScheduler),
    /// Build Faasten image from a source directory
    Build(Build),
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

fn get_default_server() -> Option<String> {
    let config_dir = dirs::config_dir()
        .unwrap_or("~/.config".into())
        .join("fstn");
    std::fs::create_dir_all(&config_dir).ok()?;
    let credentials_file = config_dir.join("credentials");
    let creds: Value = toml::from_slice(&std::fs::read(credentials_file).ok()?).ok()?;
    if let Some(server) = creds
        .get("global")
        .and_then(|v| v.get("server"))
        .and_then(Value::as_str)
    {
        Some(String::from(server))
    } else if let Some(server) = creds.get("server").and_then(Value::as_str) {
        Some(String::from(server))
    } else {
       None
    }
}

const DEFAULT_SERVER: &'static str = "https://faasten.princeton.systems";
const DEFAULT_USER: &'static str = "default";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let server = cli
        .server
        .or(std::env::var("FSTN_SERVER").ok())
        .or(get_default_server())
        .unwrap_or(String::from(DEFAULT_SERVER));

    let user = cli
        .user
        .or(std::env::var("FSTN_USER").ok())
        .unwrap_or(String::from(DEFAULT_USER));
    Fstn {
        stdout: stdout(),
        stderr: StandardStream::stderr(termcolor::ColorChoice::Auto),
        client: reqwest::blocking::ClientBuilder::new().timeout(None).build()?,
        server,
        user,

    }.run(cli.command)
}

struct Fstn<O: Write> {
    stdout: O,
    stderr: StandardStream,
    client: reqwest::blocking::Client,
    server: String,
    user: String,
}

#[derive(Debug)]
struct EarlyExit;

impl std::fmt::Display for EarlyExit {
    fn fmt(&self, _formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        Ok(())
    }
}

impl std::error::Error for EarlyExit {

}

impl<O: Write> Fstn<O> {
    fn check_credential(&self) -> Result<String, std::io::Error> {
        let config_dir = dirs::config_dir()
            .unwrap_or("~/.config".into())
            .join("fstn");
        std::fs::create_dir_all(&config_dir)?;
        let credentials_file = config_dir.join("credentials");
        let creds: Value = toml::from_slice(&std::fs::read(credentials_file)?)?;
        if let Some(token) = creds
            .get(&self.server)
            .and_then(|v| v.get(&self.user))
            .and_then(Value::as_str)
        {
            Ok(String::from(token))
        } else if let Some(token) = creds.get(&self.user).and_then(Value::as_str) {
            Ok(String::from(token))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "no token found",
            ))
        }
    }

    fn save_credential(&self, user: String, token: String) -> Result<(), Box<dyn std::error::Error>> {
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
            if let Some(server_table) = t.get_mut(&self.server) {
                server_table.as_table_mut().and_then(|b| b.insert(user, Value::String(token)))
            } else {
                t.insert(
                    self.server.clone(),
                    Value::Table(toml::map::Map::from_iter([(
                        user,
                        Value::String(token),
                    )])),
                )
            }
        });
        std::fs::write(credentials_file, toml::to_string(&credentials)?)?;
        Ok(())
    }

    fn token(&mut self, command: &str) -> Result<String, Box<dyn std::error::Error>>{
        if let Ok(token) = self.check_credential() {
            Ok(token)
        } else {
            status(&mut self.stderr, &command, &"you must first login")?;
            Err(EarlyExit.into())
        }
    }

    fn invoke(&mut self, function: String, payload: String) -> Result<Response, Box<dyn std::error::Error>> {
        let token = self.token("invoke")?;
        let mut url = Url::parse(format!("{}/faasten/invoke", self.server).as_str())?;
        url.path_segments_mut().map_err(|_| "cannot be base")?.push(&function);
        let result = self.client
            .post(url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(payload)
            .send()?;
        if result.status().is_success() {
            status(&mut self.stderr, &"Invoke", &"OK")?;
            Ok(result)
        } else {
            status(&mut self.stderr, &"Invoke", &format!("{}", result.status()))?;
            Ok(result)
        }
    }

    fn run(&mut self, action: Action) -> Result<(), Box<dyn std::error::Error>> {
        match action {
            Action::Login => {
                write!(self.stdout,
                    "Please paste the API Token found by logging in at {}/login/cas below\n> ",
                    self.server
                )?;
                self.stdout.flush()?;
                if let Some(Ok(token)) = stdin().lock().lines().next() {
                    self.save_credential(self.user.clone(), token)?;
                    status(&mut self.stderr, &"Login", &"saved")?;
                }
            }
            Action::Whoami => {
                let token = self.token("whoami")?;
                let url = Url::parse(format!("{}/me", self.server).as_str())?;
                let mut result = self.client
                    .get(url)
                    .bearer_auth(&token)
                    .header("content-type", "application/json")
                    .send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut stdout())?;
                    status(&mut self.stderr, &"Whoami", &"OK")?;
                } else {
                    status(&mut self.stderr, &"Whoami", &format!("{}", result.status()))?;
                    result.copy_to(&mut stdout())?;
                }
            }
            Action::Invoke(Invoke { function, payload }) => {
                let payload = if let Some(p) = payload {
                    p
                } else {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    buf
                };
                self.invoke(function, payload)?.copy_to(&mut stdout())?;
            },
            Action::FS(FS { op, masquerade }) => {
                let function = if let Some(user) = masquerade {
                    format!("home:<{},{}>:fsutil", user, user)
                } else {
                    "~:fsutil".into()
                };
                match op {
                    FsOp::Ping => {
                        let start = std::time::SystemTime::now();
                        let payload = serde_json::json!({"op": "ping", "args": {}});
                        self.invoke(function, serde_json::to_string(&payload)?)?;
                        let elapsed = start.elapsed()?;
                        write!(self.stdout, "{:?}\n", elapsed)?;
                    },
                    FsOp::Ls(OneArg { arg: path }) => {
                        let payload = serde_json::json!({"op": "ls", "args": { "path": path.split(":").collect::<Vec<&str>>() }});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;
                    },
                    FsOp::Unlink(TwoArgs { base, name }) => {
                        let payload = serde_json::json!({"op": "unlink", "args": {
                            "base": base.split(":").collect::<Vec<&str>>(),
                            "name": name,
                        }});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;
                    },
                    FsOp::Mkdir(TwoArgsLabel { label, base, name }) => {
                        let payload = serde_json::json!({"op": "mkdir", "args": {
                            "base": base.split(":").collect::<Vec<&str>>(),
                            "name": name,
                            "label": label.unwrap_or("T,T".into()),
                        }});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;
                    },
                    FsOp::Mkfile(TwoArgsLabel { label, base, name }) => {
                        let payload = serde_json::json!({"op": "mkfile", "args": {
                            "base": base.split(":").collect::<Vec<&str>>(),
                            "name": name,
                            "label": label.unwrap_or("T,T".into()),
                        }});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;
                    },
                    FsOp::Write(OneArg { arg: path }) => {
                        let mut data = Vec::new();
                        stdin().read_to_end(&mut data)?;

                        #[serde_as]
                        #[derive(Serialize)]
                        struct WriteArgs<'a> {
                            path: Vec<&'a str>,
                            #[serde_as(as="Base64")]
                            data: Vec<u8>
                        }

                        let payload = serde_json::json!({"op": "write", "args": WriteArgs {
                            path: path.split(":").collect::<Vec<&str>>(),
                            data: data,
                        }});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;
                    },
                    FsOp::Read(OneArg { arg: path }) => {
                        let payload = serde_json::json!({"op": "read", "args": {
                            "path": path.split(":").collect::<Vec<&str>>(),
                        }});

                        #[serde_as]
                        #[derive(Deserialize)]
                        struct ReadResult {
                            success: bool,
                            #[serde_as(as="Base64")]
                            value: Vec<u8>
                        }

                        let result: ReadResult = self.invoke(function, serde_json::to_string(&payload)?)?.json()?;
                        if result.success {
                            self.stdout.write_all(&result.value)?;
                        } else {
                            self.stderr.write_all(b"Not found")?;
                            Err(EarlyExit)?;
                        }
                    }
                    FsOp::Mkgate(MkGateArgs { label, privilege, clearance, base, name, memory, kernel, runtime, gate, app_image }) => {

                        #[derive(Debug, Serialize, Deserialize)]
                        struct MkgateArgs {
                            label: String,
                            privilege: String,
                            clearance: String,
                            base: Vec<String>,
                            name: String,
                            memory: Option<u64>,
                            app_image: Option<Vec<String>>,
                            kernel: Option<Vec<String>>,
                            runtime: Option<Vec<String>>,
                            gate: Option<Vec<String>>,
                        }

                        let mut args = MkgateArgs {
                            label,
                            privilege,
                            clearance,
                            base: base.split(":").map(ToString::to_string).collect(),
                            name,
                            memory,
                            app_image: None,
                            kernel: None,
                            runtime: None,
                            gate: gate.map(|g| g.split(":").map(ToString::to_string).collect()),
                        };

                        let mut form = reqwest::blocking::multipart::Form::new();

                        if let Some(app_image) = app_image {
                            if let Some(local_app) = app_image.strip_prefix("@") {
                                form = form.part("blob", reqwest::blocking::multipart::Part::file(local_app)?
                                                    .mime_str("application/octet-stream")?
                                                    .file_name("app_image"));
                            } else {
                                args.app_image = Some(app_image.split(":").map(ToString::to_string).collect());
                            }
                        }

                        if let Some(kernel) = kernel {
                            if let Some(local_kernel) = kernel.strip_prefix("@") {
                                form = form.part("blob", reqwest::blocking::multipart::Part::file(local_kernel)?
                                                    .mime_str("application/octet-stream")?
                                                    .file_name("kernel"));
                            } else {
                                args.kernel = Some(kernel.split(":").map(ToString::to_string).collect());
                            }
                        }

                        if let Some(runtime) = runtime {
                            if let Some(local_runtime) = runtime.strip_prefix("@") {
                                form = form.part("blob", reqwest::blocking::multipart::Part::file(local_runtime)?
                                                    .mime_str("application/octet-stream")?
                                                    .file_name("runtime"));
                            } else {
                                args.runtime = Some(runtime.split(":").map(ToString::to_string).collect());
                            }
                        }

                        let payload = serde_json::json!({
                            "op": "mkgate",
                            "args": args,
                        });

                        form = form.text("payload", serde_json::to_string(&payload)?);
                        let token = self.token("invoke")?;
                        let mut url = Url::parse(format!("{}/faasten/invoke", self.server).as_str())?;
                        url.path_segments_mut().map_err(|_| "cannot be base")?.push(&function);
                        let mut result = self.client
                            .post(url)
                            .bearer_auth(&token)
                            .multipart(form)
                            .send()?;
                        if result.status().is_success() {
                            status(&mut self.stderr, &"Invoke", &"OK")?;
                            result.copy_to(&mut self.stdout)?;
                        } else {
                            status(&mut self.stderr, &"Invoke", &format!("{}", result.status()))?;
                            result.copy_to(&mut self.stderr)?;
                        }
                    },
                    FsOp::Upgate(UpgateArgs { privilege, clearance, memory, app_image, kernel, runtime, gate, path }) => {
                        #[derive(Debug, Serialize, Deserialize)]
                        struct UpgatePayload {
                            privilege: Option<String>,
                            clearance: Option<String>,
                            memory: Option<u64>,
                            app_image: Option<Vec<String>>,
                            kernel: Option<Vec<String>>,
                            runtime: Option<Vec<String>>,
                            gate: Option<Vec<String>>,
                            path: Vec<String>,
                        }

                        let mut args = UpgatePayload {
                            privilege,
                            clearance,
                            memory,
                            app_image: None,
                            kernel: None,
                            runtime: None,
                            gate: gate.map(|g| g.split(":").map(ToString::to_string).collect()),
                            path: path.split(":").map(ToString::to_string).collect(),
                        };


                        let mut form = reqwest::blocking::multipart::Form::new();

                        if let Some(app_image) = app_image {
                            if let Some(local_app) = app_image.strip_prefix("@") {
                                form = form.part("blob", reqwest::blocking::multipart::Part::file(local_app)?
                                                    .mime_str("application/octet-stream")?
                                                    .file_name("app_image"));
                            } else {
                                args.app_image = Some(app_image.split(":").map(ToString::to_string).collect());
                            }
                        }

                        if let Some(kernel) = kernel {
                            if let Some(local_kernel) = kernel.strip_prefix("@") {
                                form = form.part("blob", reqwest::blocking::multipart::Part::file(local_kernel)?
                                                    .mime_str("application/octet-stream")?
                                                    .file_name("kernel"));
                            } else {
                                args.kernel = Some(kernel.split(":").map(ToString::to_string).collect());
                            }
                        }

                        if let Some(runtime) = runtime {
                            if let Some(local_runtime) = runtime.strip_prefix("@") {
                                form = form.part("blob", reqwest::blocking::multipart::Part::file(local_runtime)?
                                                    .mime_str("application/octet-stream")?
                                                    .file_name("runtime"));
                            } else {
                                args.runtime = Some(runtime.split(":").map(ToString::to_string).collect());
                            }
                        }

                        let payload = serde_json::json!({
                            "op": "upgate",
                            "args": args,
                        });

                        form = form.text("payload", serde_json::to_string(&payload)?);
                        let token = self.token("invoke")?;
                        let mut url = Url::parse(format!("{}/faasten/invoke", self.server).as_str())?;
                        url.path_segments_mut().map_err(|_| "cannot be base")?.push(&function);
                        let mut result = self.client
                            .post(url)
                            .bearer_auth(&token)
                            .multipart(form)
                            .send()?;
                        if result.status().is_success() {
                            status(&mut self.stderr, &"Invoke", &"OK")?;
                            result.copy_to(&mut self.stdout)?;
                        } else {
                            status(&mut self.stderr, &"Invoke", &format!("{}", result.status()))?;
                            result.copy_to(&mut self.stderr)?;
                        }
                    },
                    FsOp::Mkblob(MkBlobArgs { label, base, files }) => {
                        let payload = serde_json::json!({
                            "op": "mkblob",
                            "args": {
                                "label": label.unwrap_or("T,T".into()),
                                "base": base.split(":").collect::<Vec<&str>>(),
                            }
                        });
                        let mut form = reqwest::blocking::multipart::Form::new()
                            .text("payload", serde_json::to_string(&payload)?);

                        for file in files {
                            let file_name = std::path::Path::new(&file)
                                .file_name()
                                .and_then(|f| f.to_str())
                                .map(|f| f.to_string()).expect("File name");
                            form = form.part("blob", reqwest::blocking::multipart::Part::file(file)?
                                             .mime_str("application/octet-stream")?
                                             .file_name(file_name));
                        }
                        let token = self.token("invoke")?;
                        let mut url = Url::parse(format!("{}/faasten/invoke", self.server).as_str())?;
                        url.path_segments_mut().map_err(|_| "cannot be base")?.push(&function);
                        let mut result = self.client
                            .post(url)
                            .bearer_auth(&token)
                            .multipart(form)
                            .send()?;
                        if result.status().is_success() {
                            status(&mut self.stderr, &"Invoke", &"OK")?;
                            result.copy_to(&mut self.stdout)?;
                        } else {
                            status(&mut self.stderr, &"Invoke", &format!("{}", result.status()))?;
                            result.copy_to(&mut self.stderr)?;
                        }
                    },
                    FsOp::Cat(OneArg { arg: path }) => {
                        let payload = serde_json::json!({"op": "cat", "args": {
                            "path": path.split(":").collect::<Vec<&str>>(),
                        }});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;
                    }
                    FsOp::Mkfaceted(TwoArgs { base, name }) => {
                        let payload = serde_json::json!({"op": "mkfaceted", "args": {
                            "base": base.split(":").collect::<Vec<&str>>(),
                            "name": name,
                        }});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;
                    },
                    FsOp::Mksvc(TwoArgsLabel { base, name, label }) => {
                        #[derive(Serialize_repr, Deserialize, PartialEq, Debug)]
                        #[repr(u8)]
                        enum Verb {
                            HEAD = 0,
                            GET = 1,
                            POST = 2,
                            PUT = 3,
                            DELETE = 4,
                        }

                        #[serde_as]
                        #[derive(Debug, Serialize, Deserialize)]
                        struct MkSvc {
                            base: Option<Vec<String>>,
                            name: Option<String>,
                            label: Option<String>,
                            privilege: String,
                            clearance: String,
                            taint: String,
                            url: String,
                            verb: Verb,
                            headers: std::collections::HashMap<String, String>,
                        }

                        let mut mksvc: MkSvc = serde_json::from_reader(stdin())?;

                        mksvc.base = Some(base.split(":").map(ToString::to_string).collect());
                        mksvc.name = Some(name);
                        mksvc.label = label;

                        let payload = serde_json::json!({"op": "mksvc", "args": mksvc});
                        self.invoke(function, serde_json::to_string(&payload)?)?.copy_to(&mut self.stdout)?;

                    },
                    FsOp::Invoke(InvokeArgs { path, params }) => {
                        let mut data = Vec::new();
                        stdin().read_to_end(&mut data)?;

                        let params = params.iter().map(Clone::clone).collect();

                        #[serde_as]
                        #[derive(Serialize)]
                        struct InvokeArgs<'a> {
                            path: Vec<&'a str>,
                            sync: bool,
                            #[serde_as(as="Base64")]
                            payload: Vec<u8>,
                            params: HashMap<String, String>
                        }

                        #[serde_as]
                        #[derive(Deserialize)]
                        struct InvokeResult {
                            #[allow(dead_code)]
                            success: Option<bool>,
                            #[serde_as(as="Base64")]
                            #[serde(default)]
                            data: Option<Vec<u8>>,
                            error: Option<serde_json::Value>,
                        }

                        let payload = serde_json::json!({"op": "invoke", "args": InvokeArgs {
                            path: path.split(":").collect::<Vec<&str>>(),
                            sync: true,
                            payload: data,
                            params,
                        }});
                        let result: InvokeResult = self.invoke(function, serde_json::to_string(&payload)?)?.json()?;
                        if let Some(data) = result.data {
                            self.stdout.write_all(&data)?;
                        } else {
                            self.stderr.write_all(&serde_json::to_vec(&result.error)?)?;
                        }
                    }
                };
            },
            Action::Delegate(Delegate { save, privilege, bootstrap, clearance }) => {
                if let Ok(token) = self.check_credential() {
                    let url = Url::parse(format!("{}/faasten/delegate", self.server).as_str())?;
                    let mut result = self.client
                        .post(url)
                        .bearer_auth(&token)
                        .header("content-type", "application/json")
                        .json(&serde_json::json!({
                            "component": privilege,
                            "bootstrap": bootstrap,
                            "clearance": clearance,
                        }))
                        .send()?;
                    if result.status().is_success() {
                        let mut token = String::new();
                        result.read_to_string(&mut token)?;
                        self.stdout.write_all(token.as_bytes())?;
                        if save {
                            self.save_credential(privilege, token)?;
                        }
                        status(&mut self.stderr, &"Delegate", &"OK")?;
                    } else {
                        status(&mut self.stderr, &"Delegate", &format!("{}", result.status()))?;
                        result.copy_to(&mut stdout())?;
                    }
                } else {
                    status(&mut self.stderr, &"Delegate", &"you must first login")?;
                }
            },
            Action::Ping(Ping {}) => {
                let now = Instant::now();
                let url = Url::parse(format!("{}/faasten/ping", self.server).as_str())?;
                let _ = self.client.get(url).send()?;
                write!(self.stdout, "ping: {:?} elapsed", now.elapsed())?;
            }
            Action::PingScheduler(PingScheduler {}) => {
                let now = Instant::now();
                let url = Url::parse(format!("{}/faasten/ping/scheduler", self.server).as_str())?;
                let _ = self.client.get(url).send()?;
                write!(self.stdout, "ping: {:?} elapsed", now.elapsed())?;
            },
            Action::Build(Build { source_dir, output }) => {
                use std::os::unix::fs::PermissionsExt;
                let mut output = std::fs::File::create(output.unwrap_or("function.img".into()))?;
                let mut fswriter = backhand::FilesystemWriter::default();
                fswriter.set_root_mode(0o555);

                fn write_dir(fs: &mut backhand::FilesystemWriter, path: PathBuf, prefix: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
                    for entry in std::fs::read_dir(path)? {
                        let entry = entry?;
                        let meta = entry.metadata()?;
                        let permissions = entry.metadata()?.permissions().mode();
                        if meta.is_file() {
                            fs.push_file(std::fs::File::open(entry.path()).unwrap(),
                                            prefix.join(entry.file_name()),
                                            NodeHeader::new(permissions as u16, 0, 0, 0)).unwrap();
                        } else if meta.is_dir() {
                            let next_prefix = prefix.join(entry.file_name());
                            fs.push_dir(next_prefix.clone(), NodeHeader::new(permissions as u16, 0, 0, 0))?;
                            write_dir(fs, entry.path(), next_prefix)?;
                        }
                    }
                    Ok(())
                }

                write_dir(&mut fswriter, source_dir, "/".into())?;

                fswriter.write(&mut output)?;
            }
        }
        Ok(())
    }
}
