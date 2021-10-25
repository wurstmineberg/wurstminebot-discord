use {
    std::{
        collections::HashMap,
        convert::Infallible as Never,
        iter,
    },
    futures::stream::TryStreamExt as _,
    minecraft::chat::Chat,
    serde::Deserialize,
    serenity::prelude::*,
    serenity_utils::RwFuture,
    sqlx::types::Json,
    systemd_minecraft::World,
    twitchchat::{
        UserConfig,
        messages::Commands,
        runner::{
            AsyncRunner,
            Status,
        },
    },
    crate::{
        Database,
        Error,
        minecraft::tellraw,
    }
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "make_wurstminebot")]
    bot_username: String,
    #[serde(rename = "clientID")]
    client_id: String,
    client_secret: String,
}

impl Config {
    async fn user_config(&self) -> Result<UserConfig, Error> {
        let api_client = twitch_helix::Client::new(
            concat!("wurstminebot/", env!("CARGO_PKG_VERSION")),
            self.client_id.clone(),
            twitch_helix::Credentials::from_client_secret(&self.client_secret, iter::empty::<String>()),
        )?;
        let cfg = UserConfig::builder()
            .name(&self.bot_username)
            .token(format!("oauth:{}", api_client.get_oauth_token(None).await?))
            .enable_all_capabilities()
            .build()?;
        Ok(cfg)
    }
}

pub async fn listen_chat(ctx_fut: RwFuture<Context>) -> Result<Never, Error> {
    loop {
        let ctx = ctx_fut.read().await;
        let data = (*ctx).data.read().await;
        let mut nick_map = HashMap::<String, String>::default();
        let pool = data.get::<Database>().expect("missing database connection");
        let mut query = sqlx::query!(r#"SELECT data->'minecraft'->'nicks'->-1 as "minecraft_nick!: Json<String>", data->'twitch'->'login' as "twitch_nick!: Json<String>" FROM people WHERE data->'minecraft'->'nicks'->-1 IS NOT NULL AND data->'twitch'->'login' IS NOT NULL"#)
            .fetch(pool);
        while let Some(person_data) = query.try_next().await? {
            nick_map.insert(person_data.twitch_nick.0, person_data.minecraft_nick.0);
        }
        let user_config = data.get::<crate::config::Config>().expect("missing config").twitch.user_config().await?;
        let connector = twitchchat::connector::tokio::Connector::twitch()?;
        let mut runner = AsyncRunner::connect(connector, &user_config).await?;
        for (twitch_nick, minecraft_nick) in &nick_map {
            runner.join(twitch_nick).await?; //TODO dynamically join/leave channels as nick map is updated
            for world in World::all_running().await? {
                tellraw(&world, minecraft_nick, Chat::from(format!("[Twitch] reconnected")).color(minecraft::chat::Color::Aqua)).await?;
            }
        }
        loop {
            match runner.next_message().await? {
                Status::Message(Commands::Privmsg(pm)) => {
                    let channel_name = &pm.channel();
                    if channel_name.starts_with('#') {
                        if let Some(minecraft_nick) = nick_map.get(&channel_name[1..]) {
                            for world in World::all_running().await? {
                                tellraw(&world, minecraft_nick, Chat::from(format!(
                                    "[Twitch] {} {}",
                                    if pm.is_action() { format!("* {}", pm.name()) } else { format!("<{}>", pm.name()) },
                                    pm.data(),
                                )).color(minecraft::chat::Color::Aqua)).await?;
                            }
                        } else {
                            return Err(Error::UnknownTwitchNick(channel_name.to_string()))
                        }
                    } else {
                        return Err(Error::MalformedTwitchChannelName(channel_name.to_string()))
                    }
                }
                Status::Message(_) => {}
                Status::Quit | Status::Eof => break,
            }
        }
    }
}

fn make_wurstminebot() -> String { format!("wurstminebot") }
