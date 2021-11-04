//! The base library for the Wurstmineberg Discord bot, wurstminebot

#![deny(rust_2018_idioms, unused, unused_crate_dependencies, unused_import_braces, unused_lifetimes, unused_qualifications, warnings)]
#![forbid(unsafe_code)]

use {
    std::{
        env,
        fmt,
        io,
        path::Path,
    },
    derive_more::From,
    sqlx::PgPool,
    serenity::{
        model::prelude::*,
        prelude::*,
    },
};

pub mod cal;
pub mod commands;
pub mod config;
pub mod emoji;
pub mod http;
pub mod ipc;
#[cfg(unix)] pub mod log;
pub mod minecraft;
pub mod parse;
pub mod people;
pub mod twitch;
mod util;

/// The address and port where the bot listens for IPC commands.
pub const IPC_ADDR: &str = "127.0.0.1:18809";

/// The guild ID for the Wurstmineberg guild.
pub const WURSTMINEBERG: GuildId = GuildId(88318761228054528);

pub(crate) const GENERAL: ChannelId = ChannelId(88318761228054528);
pub const DEV: ChannelId = ChannelId(506905544901001228);

/// The directory where all Wurstmineberg-related files are located: `/opt/wurstmineberg`.
pub fn base_path() -> &'static Path { //TODO make this a constant when stable
    Path::new("/opt/wurstmineberg")
}

/// Errors that may occur in this crate.
#[derive(Debug, From)]
pub enum Error {
    Annotated(String, Box<Error>),
    ChannelIdParse(ChannelIdParseError),
    Envar(env::VarError),
    Io(io::Error),
    Ipc(crate::ipc::Error),
    Join(tokio::task::JoinError),
    Json(serde_json::Error),
    #[cfg(unix)]
    Log(log::Error),
    #[from(ignore)]
    MalformedTwitchChannelName(String),
    Minecraft(systemd_minecraft::Error),
    /// Returned if a Serenity context was required outside of an event handler but the `ready` event has not been received yet.
    MissingContext,
    /// Returned by the user list handler if a user has no join date.
    MissingJoinDate,
    Serenity(serenity::Error),
    Sql(sqlx::Error),
    Twitch(twitch_helix::Error),
    #[from(ignore)]
    UnknownTwitchNick(String),
    UserIdParse(UserIdParseError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Annotated(msg, e) => write!(f, "{}: {}", msg, e),
            Error::ChannelIdParse(e) => e.fmt(f),
            Error::Envar(e) => e.fmt(f),
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::Ipc(e) => e.fmt(f),
            Error::Join(e) => e.fmt(f),
            Error::Json(e) => write!(f, "JSON error: {}", e),
            #[cfg(unix)]
            Error::Log(e) => e.fmt(f),
            Error::MalformedTwitchChannelName(channel_name) => write!(f, "IRC channel name \"{}\" doesn't start with \"#\"", channel_name),
            Error::Minecraft(e) => e.fmt(f),
            Error::MissingContext => write!(f, "Serenity context not available before ready event"),
            Error::MissingJoinDate => write!(f, "encountered user without join date"),
            Error::Serenity(e) => e.fmt(f),
            Error::Sql(e) => e.fmt(f),
            Error::Twitch(e) => e.fmt(f),
            Error::UnknownTwitchNick(channel_name) => write!(f, "no Minecraft nick matching Twitch nick \"{}\"", channel_name),
            Error::UserIdParse(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

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
    type Value = PgPool;
}
