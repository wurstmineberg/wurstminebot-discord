//! The base library for the Wurstmineberg Discord bot, wurstminebot

use {
    std::{
        env,
        io,
        path::Path,
    },
    sqlx::PgPool,
    serenity::{
        model::prelude::*,
        prelude::*,
    },
};

pub mod cal;
pub mod config;
pub mod http;
pub mod ipc;
pub mod log;
pub mod minecraft;
pub mod parse;
pub mod people;
pub mod twitch;
mod util;

/// The address and port where the bot listens for IPC commands.
pub const IPC_ADDR: &str = "127.0.0.1:18809";

/// The guild ID for the Wurstmineberg guild.
pub const WURSTMINEBERG: GuildId = GuildId::new(88318761228054528);

pub const GENERAL: ChannelId = ChannelId::new(88318761228054528);
pub const DEV: ChannelId = ChannelId::new(506905544901001228);

/// The directory where all Wurstmineberg-related files are located: `/opt/wurstmineberg`.
pub fn base_path() -> &'static Path { //TODO make this a constant when stable
    #[cfg(unix)] { Path::new("/opt/wurstmineberg") }
    #[cfg(windows)] { Path::new("C:/Users/fenhl/games/minecraft/srv/wmb") } // for testing
}

/// Errors that may occur in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)] Envar(#[from] env::VarError),
    #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Ipc(#[from] crate::ipc::Error),
    #[error(transparent)] Join(#[from] tokio::task::JoinError),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] Log(#[from] log::Error),
    #[error(transparent)] Minecraft(#[from] systemd_minecraft::Error),
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Twitch(#[from] twitch_helix::Error),
    #[error(transparent)] TwitchValidate(#[from] twitch_irc::validate::Error),
    #[error("{0}: {1}")]
    Annotated(String, Box<Error>),
    #[error("IRC channel name \"{0}\" doesn't start with \"#\"")]
    MalformedTwitchChannelName(String),
    #[error("encountered user without join date")]
    MissingJoinDate,
    #[error("no Minecraft nick matching Twitch nick \"{0}\"")]
    UnknownTwitchNick(String),
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
    type Value = PgPool;
}
