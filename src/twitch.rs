use {
    std::{
        collections::HashMap,
        convert::Infallible as Never
    },
    futures::stream::StreamExt as _,
    systemd_minecraft::World,
    twitchchat::{
        Dispatcher,
        RateLimit,
        Runner,
        events
    },
    crate::{
        Error,
        minecraft::{
            self,
            Chat
        },
        people::Person
    }
};

pub async fn listen_chat(world: World, members: impl IntoIterator<Item = Person>) -> Result<Never, Error> {
    let nick_map = members.into_iter()
        .filter_map(|member| Some((member.twitch_nick()?.to_string(), member.minecraft_nick()?.to_string())))
        .collect::<HashMap<_, _>>();
    let (nick, token) = twitchchat::ANONYMOUS_LOGIN;
    let dispatcher = Dispatcher::new();
    let (runner, mut control) = Runner::new(dispatcher.clone(), RateLimit::default());
    let conn = twitchchat::connect_easy_tls(nick, token).await?;
    let done = runner.run(conn);
    let mut events = dispatcher.subscribe::<events::Privmsg>();
    dispatcher.wait_for::<events::IrcReady>().await?;
    for (twitch_nick, _) in &nick_map {
        control.writer().join(twitch_nick).await?; //TODO dynamically join/leave channels as nick map is updated
    }
    while let Some(msg) = events.next().await {
        let channel_name = &msg.channel;
        if channel_name.starts_with('#') {
            if let Some(minecraft_nick) = nick_map.get(&channel_name[1..]) {
                minecraft::tellraw(&world, minecraft_nick, Chat::from(format!(
                    "{} {}",
                    format!("<twitch:{}>", msg.name), //if msg.is_action() { format!("* twitch:{}", msg.name) } else { format!("<twitch:{}>", msg.name) }, //TODO https://github.com/museun/twitchchat/issues/120
                    msg.data
                )).color(minecraft::Color::Aqua))?;
            } else {
                return Err(Error::UnknownTwitchNick(channel_name.to_string()));
            }
        } else {
            return Err(Error::MalformedTwitchChannelName(channel_name.to_string()));
        }
    }
    Err(Error::TwitchClientTerminated(done.await?))
}
