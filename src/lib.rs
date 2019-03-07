//! The base library for the Wurstmineberg Discord bot, wurstminebot

#![cfg_attr(test, deny(warnings))]
#![warn(trivial_casts)]
#![deny(unused, missing_docs, unused_qualifications)]
#![deny(rust_2018_idioms)] // this badly-named lint actually produces errors when Rust 2015 idioms are used
#![forbid(unused_import_braces)]

use std::{
    fs::File,
    io,
    path::Path,
    sync::Arc
};
use serde_derive::Deserialize;
use serenity::{
    client::bridge::gateway::ShardManager,
    prelude::*
};
use typemap::Key;
use wrapped_enum::wrapped_enum;

pub mod voice;

/// The directory where all Wurstmineberg-related files are located: `/opt/wurstmineberg`.
pub fn base_path() -> &'static Path { //TODO make this a constant when stable
    Path::new("/opt/wurstmineberg")
}

wrapped_enum! {
    /// Errors that may occur in this crate.
    #[derive(Debug)]
    pub enum Error {
        #[allow(missing_docs)]
        Io(io::Error),
        #[allow(missing_docs)]
        SerDe(serde_json::Error),
        #[allow(missing_docs)]
        Serenity(serenity::Error)
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

/// Utility function to shut down all shards.
pub fn shut_down(ctx: &Context) {
    ctx.invisible(); // hack to prevent the bot showing as online when it's not
    let data = ctx.data.lock();
    let mut shard_manager = data.get::<ShardManagerContainer>().expect("missing shard manager").lock();
    shard_manager.shutdown_all();
}
