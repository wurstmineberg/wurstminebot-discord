//! The base library for the Wurstmineberg Discord bot, wurstminebot

#![deny(rust_2018_idioms, unused, unused_crate_dependencies, unused_import_braces, unused_lifetimes, unused_qualifications, warnings)]
#![forbid(unsafe_code)]

#[macro_use] extern crate diesel;

use {
    std::{
        env,
        fmt,
        io,
        path::Path,
        time::Duration,
    },
    derive_more::From,
    diesel::prelude::*,
    serenity::{
        model::prelude::*,
        prelude::*,
    },
    serenity_utils::RwFuture,
};

pub mod commands;
pub mod config;
pub mod emoji;
pub mod ipc;
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

const DEV: ChannelId = ChannelId(506905544901001228);

/// The directory where all Wurstmineberg-related files are located: `/opt/wurstmineberg`.
pub fn base_path() -> &'static Path { //TODO make this a constant when stable
    Path::new("/opt/wurstmineberg")
}

/// Errors that may occur in this crate.
#[derive(Debug, From)]
pub enum Error {
    Annotated(String, Box<Error>),
    ChannelIdParse(ChannelIdParseError),
    Diesel(diesel::result::Error),
    DieselConnection(ConnectionError),
    Envar(env::VarError),
    Io(io::Error),
    Ipc(crate::ipc::Error),
    Join(tokio::task::JoinError),
    Json(serde_json::Error),
    Log(log::Error),
    #[from(ignore)]
    MalformedTwitchChannelName(String),
    Minecraft(systemd_minecraft::Error),
    /// Returned if a Serenity context was required outside of an event handler but the `ready` event has not been received yet.
    MissingContext,
    /// Returned by the user list handler if a user has no join date.
    MissingJoinDate,
    Serenity(serenity::Error),
    Twitch(twitch_helix::Error),
    TwitchRunner(twitchchat::RunnerError),
    TwitchUserConfig(twitchchat::twitch::UserConfigError),
    #[from(ignore)]
    UnknownTwitchNick(String),
    UserIdParse(UserIdParseError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Annotated(msg, e) => write!(f, "{}: {}", msg, e),
            Error::ChannelIdParse(e) => e.fmt(f),
            Error::Diesel(e) => e.fmt(f),
            Error::DieselConnection(e) => e.fmt(f),
            Error::Envar(e) => e.fmt(f),
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::Ipc(e) => e.fmt(f),
            Error::Join(e) => e.fmt(f),
            Error::Json(e) => write!(f, "JSON error: {}", e),
            Error::Log(e) => e.fmt(f),
            Error::MalformedTwitchChannelName(channel_name) => write!(f, "IRC channel name \"{}\" doesn't start with \"#\"", channel_name),
            Error::Minecraft(e) => e.fmt(f),
            Error::MissingContext => write!(f, "Serenity context not available before ready event"),
            Error::MissingJoinDate => write!(f, "encountered user without join date"),
            Error::Serenity(e) => e.fmt(f),
            Error::Twitch(e) => e.fmt(f),
            Error::TwitchRunner(e) => write!(f, "Twitch chat error: {}", e),
            Error::TwitchUserConfig(e) => write!(f, "error generating Twitch chat user config: {}", e),
            Error::UnknownTwitchNick(channel_name) => write!(f, "no Minecraft nick matching Twitch nick \"{}\"", channel_name),
            Error::UserIdParse(e) => e.fmt(f),
        }
    }
}

/// A helper trait for annotating errors with more informative error messages.
pub trait IntoResultExt {
    /// The return type of the `annotate` method.
    type T;

    /// Annotates an error with an additional message which is displayed along with the error.
    fn annotate(self, msg: impl ToString) -> Self::T;
}

impl<E: Into<Error>> IntoResultExt for E {
    type T = Error;

    fn annotate(self, note: impl ToString) -> Error {
        Error::Annotated(note.to_string(), Box::new(self.into()))
    }
}

impl<T, E: IntoResultExt> IntoResultExt for Result<T, E> {
    type T = Result<T, E::T>;

    fn annotate(self, note: impl ToString) -> Result<T, E::T> {
        self.map_err(|e| e.annotate(note))
    }
}

/// `typemap` key for the PostgreSQL database connection.
pub struct Database;

impl TypeMapKey for Database {
    type Value = Mutex<PgConnection>;
}

pub async fn notify_thread_crash(ctx: RwFuture<Context>, thread_kind: String, e: impl Into<Error>, auto_retry: Option<Duration>) {
    let ctx = ctx.read().await;
    let e = e.into();
    DEV.say(&*ctx, format!(
        "{} thread crashed: {} (`{:?}`), {}",
        thread_kind,
        e,
        e,
        if let Some(auto_retry) = auto_retry { format!("auto-retrying in `{:?}`", auto_retry) } else { format!("**not** auto-retrying") },
    )).await.expect("failed to send thread crash notification");
}
