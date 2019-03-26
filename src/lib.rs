//! The base library for the Wurstmineberg Discord bot, wurstminebot

#![cfg_attr(test, deny(warnings))]
#![warn(trivial_casts)]
#![deny(unused, missing_docs, unused_import_braces, unused_qualifications)]
#![deny(rust_2018_idioms)] // this badly-named lint actually produces errors when Rust 2015 idioms are used

#[macro_use] extern crate diesel;

use std::{
    fmt,
    fs::File,
    io::{
        self,
        BufReader,
        prelude::*
    },
    net::TcpStream,
    path::Path,
    sync::Arc
};
use diesel::prelude::*;
use serde_derive::Deserialize;
use serenity::{
    client::bridge::gateway::ShardManager,
    prelude::*
};
use typemap::Key;
use wrapped_enum::wrapped_enum;

pub mod commands;
pub mod emoji;
pub mod parse;
pub mod people;
pub mod schema;
pub mod voice;

/// The address and port where the bot listens for IPC commands.
pub const IPC_ADDR: &str = "127.0.0.1:18809";

/// The directory where all Wurstmineberg-related files are located: `/opt/wurstmineberg`.
pub fn base_path() -> &'static Path { //TODO make this a constant when stable
    Path::new("/opt/wurstmineberg")
}

/// A collection of possible errors not simply forwarded from other libraries.
#[derive(Debug)]
pub enum OtherError {
    /// Returned if a Serenity context was required outside of an event handler but the `ready` event has not been received yet.
    MissingContext,
    /// Returned by the user list handler if a user has no join date.
    MissingJoinDate,
    /// The reply to an IPC command did not end in a newline.
    MissingNewline,
    /// Returned from `listen_ipc` if a command line was not valid shell lexer tokens.
    Shlex,
    /// Returned from `listen_ipc` if an unknown command is received.
    UnknownCommand(Vec<String>)
}

impl fmt::Display for OtherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            OtherError::MissingContext => write!(f, "Serenity context not available before ready event"),
            OtherError::MissingJoinDate => write!(f, "encountered user without join date"),
            OtherError::MissingNewline => write!(f, "the reply to an IPC command did not end in a newline"),
            OtherError::Shlex => write!(f, "failed to parse IPC command line"),
            OtherError::UnknownCommand(ref args) => write!(f, "unknown command: {:?}", args)
        }
    }
}

wrapped_enum! {
    /// Errors that may occur in this crate.
    #[derive(Debug)]
    pub enum Error {
        #[allow(missing_docs)]
        Diesel(diesel::result::Error),
        #[allow(missing_docs)]
        Io(io::Error),
        #[allow(missing_docs)]
        Other(OtherError),
        #[allow(missing_docs)]
        SerDe(serde_json::Error),
        #[allow(missing_docs)]
        Serenity(serenity::Error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Diesel(ref e) => e.fmt(f),
            Error::Io(ref e) => e.fmt(f),
            Error::Other(ref e) => e.fmt(f),
            Error::SerDe(ref e) => e.fmt(f),
            Error::Serenity(ref e) => e.fmt(f)
        }
    }
}

/// A parsed configuration file for wurstminebot.
#[derive(Deserialize)]
pub struct Config {
    wurstminebot: ConfigWurstminebot
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigWurstminebot {
    bot_token: String
}

impl Config {
    /// Read `/opt/wurstmineberg/config.json` and return it as a `Config`.
    pub fn new() -> Result<Config, Error> {
        Ok(serde_json::from_reader(File::open(base_path().join("config.json"))?)?)
    }

    /// Returns the Discord bot token specified in the config.
    pub fn token(&self) -> &str {
        &self.wurstminebot.bot_token
    }
}

/// `typemap` key for the serenity shard manager.
pub struct ShardManagerContainer;

impl Key for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

/// `typemap` key for the PostgreSQL database connection.
pub struct Database;

impl Key for Database {
    type Value = Mutex<PgConnection>;
}

/// Sends an IPC command to the bot.
///
/// **TODO:** document available IPC commands
pub fn send_ipc_command<T: fmt::Display, I: IntoIterator<Item = T>>(cmd: I) -> Result<String, Error> {
    let mut stream = TcpStream::connect(IPC_ADDR)?;
    writeln!(&mut stream, "{}", cmd.into_iter().map(|arg| shlex::quote(&arg.to_string()).into_owned()).collect::<Vec<_>>().join(" "))?;
    let mut buf = String::default();
    BufReader::new(stream).read_line(&mut buf)?;
    if buf.pop() != Some('\n') { return Err(OtherError::MissingNewline.into()) }
    Ok(buf)
}

/// Utility function to shut down all shards.
pub fn shut_down(ctx: &Context) {
    ctx.invisible(); // hack to prevent the bot showing as online when it's not
    let data = ctx.data.lock();
    let mut shard_manager = data.get::<ShardManagerContainer>().expect("missing shard manager").lock();
    shard_manager.shutdown_all();
}
