//! The base library for the Wurstmineberg Discord bot, wurstminebot

#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

#[macro_use] extern crate diesel;

use {
    std::{
        collections::{
            BTreeMap,
            BTreeSet
        },
        env,
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
    },
    derive_more::From,
    diesel::prelude::*,
    serde::Deserialize,
    serenity::{
        client::bridge::gateway::ShardManager,
        model::prelude::*,
        prelude::*
    },
    typemap::Key
};

pub mod commands;
pub mod emoji;
pub mod log;
pub mod minecraft;
pub mod parse;
pub mod people;
pub mod schema;
pub mod twitch;
mod util;
pub mod voice;

/// The address and port where the bot listens for IPC commands.
pub const IPC_ADDR: &str = "127.0.0.1:18809";

/// The guild ID for the Wurstmineberg guild.
pub const WURSTMINEBERG: GuildId = GuildId(88318761228054528);

/// The directory where all Wurstmineberg-related files are located: `/opt/wurstmineberg`.
pub fn base_path() -> &'static Path { //TODO make this a constant when stable
    Path::new("/opt/wurstmineberg")
}

/// Errors that may occur in this crate.
#[derive(Debug, From)]
pub enum Error {
    Diesel(diesel::result::Error),
    DieselConnection(ConnectionError),
    Envar(env::VarError),
    Io(io::Error),
    Join(tokio::task::JoinError),
    Log(log::Error),
    #[from(ignore)]
    MalformedTwitchChannelName(String),
    Minecraft(systemd_minecraft::Error),
    /// Returned if a Serenity context was required outside of an event handler but the `ready` event has not been received yet.
    MissingContext,
    /// Returned by the user list handler if a user has no join date.
    MissingJoinDate,
    /// The reply to an IPC command did not end in a newline.
    MissingNewline,
    SerDe(serde_json::Error),
    Serenity(serenity::Error),
    /// Returned from `listen_ipc` if a command line was not valid shell lexer tokens.
    Shlex,
    Twitch(twitchchat::Error),
    #[from(ignore)]
    TwitchClientTerminated(twitchchat::Status),
    TwitchEventStreamEnded,
    #[from(ignore)]
    /// Returned from `listen_ipc` if an unknown command is received.
    UnknownCommand(Vec<String>),
    #[from(ignore)]
    UnknownTwitchNick(String),
    UserIdParse(UserIdParseError),
    Wrapped((String, Box<Error>))
}

/// A helper trait for annotating errors with more informative error messages.
pub trait IntoResult<T> {
    /// Annotates an error with an additional message which is displayed along with the error.
    fn annotate(self, msg: impl Into<String>) -> Result<T, Error>;
}

impl<T, E: Into<Error>> IntoResult<T> for Result<T, E> {
    fn annotate(self, msg: impl Into<String>) -> Result<T, Error> {
        self.map_err(|e| Error::Wrapped((msg.into(), Box::new(e.into()))))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Diesel(ref e) => e.fmt(f),
            Error::DieselConnection(ref e) => e.fmt(f),
            Error::Envar(ref e) => e.fmt(f),
            Error::Io(ref e) => e.fmt(f),
            Error::Join(ref e) => e.fmt(f),
            Error::Log(ref e) => e.fmt(f),
            Error::MalformedTwitchChannelName(ref channel_name) => write!(f, "IRC channel name \"{}\" doesn't start with \"#\"", channel_name),
            Error::Minecraft(ref e) => e.fmt(f),
            Error::MissingContext => write!(f, "Serenity context not available before ready event"),
            Error::MissingJoinDate => write!(f, "encountered user without join date"),
            Error::MissingNewline => write!(f, "the reply to an IPC command did not end in a newline"),
            Error::SerDe(ref e) => e.fmt(f),
            Error::Serenity(ref e) => e.fmt(f),
            Error::Shlex => write!(f, "failed to parse IPC command line"),
            Error::Twitch(ref e) => e.fmt(f),
            Error::TwitchClientTerminated(status) => write!(f, "Twitch chat client unexpectedly returned from event loop with status {:?}", status),
            Error::TwitchEventStreamEnded => write!(f, "Twitch chat event stream ended unexpectedly"),
            Error::UnknownCommand(ref args) => write!(f, "unknown command: {:?}", args),
            Error::UnknownTwitchNick(ref channel_name) => write!(f, "no Minecraft nick matching Twitch nick \"{}\"", channel_name),
            Error::UserIdParse(ref e) => e.fmt(f),
            Error::Wrapped((ref msg, ref e)) => write!(f, "{}: {}", msg, e)
        }
    }
}

/// A parsed configuration file for wurstminebot.
#[derive(Deserialize)]
pub struct Config {
    pub wurstminebot: ConfigWurstminebot
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWurstminebot {
    bot_token: String,
    #[serde(default)]
    self_assignable_roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub world_channels: BTreeMap<String, ChannelId>
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

impl Key for Config {
    type Value = Config;
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
    if buf.pop() != Some('\n') { return Err(Error::MissingNewline); }
    Ok(buf)
}

/// Utility function to shut down all shards.
pub fn shut_down(ctx: &Context) {
    ctx.invisible(); // hack to prevent the bot showing as online when it's not
    let data = ctx.data.read();
    let mut shard_manager = data.get::<ShardManagerContainer>().expect("missing shard manager").lock();
    shard_manager.shutdown_all();
}
