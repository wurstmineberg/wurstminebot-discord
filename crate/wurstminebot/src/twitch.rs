use {
    std::{
        collections::HashMap,
        convert::Infallible as Never,
    },
    futures::stream::StreamExt as _,
    serenity::prelude::*,
    serenity_utils::RwFuture,
    systemd_minecraft::World,
    twitchchat::{
        Connector,
        Dispatcher,
        Runner,
        events
    },
    crate::{
        Database,
        Error,
        minecraft::{
            self,
            Chat
        },
        people::Person
    }
};

pub async fn listen_chat(ctx_fut: RwFuture<Context>) -> Result<Never, Error> {
    loop {
        let ctx = ctx_fut.read().await;
        let data = (*ctx).data.read().await;
        let conn = data.get::<Database>().expect("missing database connection").lock().await;
        let everyone = Person::all(&conn)?;
        let nick_map = everyone.into_iter()
            .filter_map(|member| Some((member.twitch_nick()?.to_string(), member.minecraft_nick()?.to_string())))
            .collect::<HashMap<_, _>>();
        let (nick, token) = twitchchat::ANONYMOUS_LOGIN;
        let dispatcher = Dispatcher::new();
        let (mut runner, mut control) = Runner::new(dispatcher.clone());
        let conn = Connector::new(move || twitchchat::rustls::connect_easy(nick, token));
        let done = runner.run_to_completion(conn); //TODO use run_with_retry instead?
        let handler = tokio::spawn(async move {
            let mut events = dispatcher.subscribe::<events::Privmsg>();
            dispatcher.wait_for::<events::IrcReady>().await?;
            for (twitch_nick, _) in &nick_map {
                control.writer().join(twitch_nick).await?; //TODO dynamically join/leave channels as nick map is updated
            }
            for world in World::all_running().await? {
                for (_, minecraft_nick) in &nick_map {
                    minecraft::tellraw(&world, minecraft_nick, Chat::from(format!("[Twitch] reconnected")).color(minecraft::Color::Aqua)).await?;
                }
            }
            while let Some(msg) = events.next().await {
                let channel_name = &msg.channel;
                if channel_name.starts_with('#') {
                    if let Some(minecraft_nick) = nick_map.get(&channel_name[1..]) {
                        for world in World::all_running().await? {
                            minecraft::tellraw(&world, minecraft_nick, Chat::from(format!(
                                "[Twitch] {} {}",
                                if msg.is_action() { format!("* {}", msg.name) } else { format!("<{}>", msg.name) },
                                msg.data
                            )).color(minecraft::Color::Aqua)).await?;
                        }
                    } else {
                        return Err(Error::UnknownTwitchNick(channel_name.to_string()));
                    }
                } else {
                    return Err(Error::MalformedTwitchChannelName(channel_name.to_string()));
                }
            }
            Err(Error::TwitchEventStreamEnded)
        });
        break tokio::select! {
            join_result = handler => join_result?,
            status = done => {
                status?;
                continue // reconnect after a network timeout or other error that causes twitchchat to return
            }
        }
    }
}
