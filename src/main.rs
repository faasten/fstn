use core::fmt;
use std::{
    collections::HashMap,
    fs::File,
    io::{stdin, BufRead, Read, Write},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use reqwest::{blocking::multipart::Form, blocking::Body, Url};
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
struct List {
    /// Database key to get
    dir: String,
}

#[derive(Parser, Debug)]
struct Get {
    /// Database key to get
    key: String,
}

#[derive(Parser, Debug)]
struct Set {
    /// Database key to set
    key: String,
    /// Value to set
    value: Option<String>,
}

#[derive(Parser, Debug)]
struct Blob {
    /// Database key pointing to the blob
    key: String,
    /// Path to a local file
    file: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Action {
    /// Login to Faasten
    Login,
    /// Get the value of a database key
    Get(Get),
    /// Set the value of a database key from the provided value or standard in
    Set(Set),
    /// Put a "blob" from a local file
    Put(Blob),
    /// Download a "blob" to a local file
    Fetch(Blob),
    List(List),
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
        Action::Get(Get { key }) => {
            if let Ok(token) = check_credential(&server) {
                let url =
                    Url::parse_with_params(format!("{}/get", server).as_str(), &[("keys", &key)])?;
                let result = client
                    .get(url)
                    .bearer_auth(&token)
                    .send()?
                    .json::<HashMap<String, Option<String>>>()?;
                if let Some(value) = result.get(&key).unwrap_or(&None) {
                    print!("{}", value);
                    status(&mut stderr, &"Get", &"OK")?;
                } else {
                    status(&mut stderr, &"Get", &format!("\"{}\" not found", key))?;
                }
            } else {
                status(&mut stderr, &"Get", &"you must first login")?;
            }
        }
        Action::List(List { dir }) => {
            if let Ok(token) = check_credential(&server) {
                let url =
                    Url::parse_with_params(format!("{}/read_dir", server).as_str(), &[("dir", &dir)])?;
                let result = client
                    .get(url)
                    .bearer_auth(&token)
                    .send()?
                    .json::<Vec<String>>()?;
                for entry in result.iter() {
                    println!("{}", entry);
                }
                status(&mut stderr, &"List", &"OK")?;
            } else {
                status(&mut stderr, &"List", &"you must first login")?;
            }
        }
        Action::Set(Set { key, value }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse(format!("{}/put", server).as_str())?;
                let value = value.unwrap_or_else(|| {
                    let mut buf = Vec::new();
                    stdin()
                        .lock()
                        .read_to_end(&mut buf)
                        .expect("couldn't read from stdin");
                    String::from_utf8_lossy(buf.as_ref()).into()
                });
                let form = Form::new().text(key, value).percent_encode_noop();
                let result = client
                    .post(url)
                    .bearer_auth(&token)
                    .multipart(form)
                    .send()?;
                if result.status().is_success() {
                    status(&mut stderr, &"Set", &"OK")?;
                } else {
                    status(&mut stderr, &"Set", &format!("{}", result.status()))?;
                }
            } else {
                status(&mut stderr, &"Set", &"you must first login")?;
            }
        }
        Action::Put(Blob { key, file }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse_with_params(
                    format!("{}/put_blob", server).as_str(),
                    &[("key", &key)],
                )?;
                let result = client
                    .post(url)
                    .bearer_auth(&token)
                    .body(Body::from(File::open(file)?))
                    .send()?;
                if result.status().is_success() {
                    status(&mut stderr, &"Put", &"OK")?;
                } else {
                    status(&mut stderr, &"Put", &format!("{}", result.status()))?;
                }
            } else {
                status(&mut stderr, &"Put", &"you must first login")?;
            }
        }
        Action::Fetch(Blob { key, file }) => {
            if let Ok(token) = check_credential(&server) {
                let url = Url::parse_with_params(
                    format!("{}/get_blob", server).as_str(),
                    &[("key", &key)],
                )?;
                let mut result = client.get(url).bearer_auth(&token).send()?;
                if result.status().is_success() {
                    std::io::copy(&mut result, &mut File::create(file)?)?;
                    status(&mut stderr, &"Fetch", &"OK")?;
                } else {
                    status(&mut stderr, &"Fetch", &format!("{}", result.status()))?;
                }
            } else {
                status(&mut stderr, &"Fetch", &"you must first login")?;
            }
        }
    }
    Ok(())
}
