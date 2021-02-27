use {
    std::collections::{
        BTreeMap,
        BTreeSet,
    },
    serde::Deserialize,
    serenity::{
        model::prelude::*,
        prelude::*,
    },
    tokio::fs,
};

/// A parsed configuration file for wurstminebot.
#[derive(Deserialize)]
pub struct Config {
    pub wurstminebot: ConfigWurstminebot,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWurstminebot {
    pub bot_token: String,
    #[serde(default)]
    pub(crate) self_assignable_roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub world_channels: BTreeMap<String, ChannelId>,
}

impl Config {
    /// Read `/opt/wurstmineberg/config.json` and return it as a `Config`.
    pub async fn new() -> Result<Config, crate::Error> {
        let buf = fs::read_to_string(crate::base_path().join("config.json")).await?;
        Ok(serde_json::from_str(&buf)?) //TODO use async-json
    }
}

impl TypeMapKey for Config {
    type Value = Config;
}
