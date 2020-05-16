use {
    std::{
        collections::HashMap,
        convert::Infallible as Never
    },
    systemd_minecraft::World,
    twitchchat::{
        Event,
        Message,
        commands
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

pub fn listen_chat(world: World, members: impl IntoIterator<Item = Person>) -> Result<Never, Error> {
    let nick_map = members.into_iter()
        .filter_map(|member| Some((member.twitch_nick()?.to_string(), member.minecraft_nick()?.to_string())))
        .collect::<HashMap<_, _>>();
    let (nick, token) = twitchchat::ANONYMOUS_LOGIN;
    let client = twitchchat::connect_easy(nick, token)?.filter::<commands::PrivMsg>();
    let writer = client.writer();
    for event in client {
        match event {
            Event::IrcReady(_) => {
                println!("Twitch connected");
                for (twitch_nick, _) in &nick_map {
                    writer.join(twitch_nick)?;
                }
            }
            Event::Message(Message::Irc(msg)) => {
                println!("IRC received {:?}", msg);
            }
            Event::Message(Message::PrivMsg(msg)) => {
                let channel_name = msg.channel().as_str();
                if channel_name.starts_with('#') {
                    if let Some(minecraft_nick) = nick_map.get(&channel_name[1..]) {
                        minecraft::tellraw(&world, minecraft_nick, Chat::from(format!(
                            "{} {}",
                            if msg.is_action() {
                                format!("* twitch:{}", msg.user())
                            } else {
                                format!("<twitch:{}>", msg.user())
                            },
                            msg.message()
                        )).color(minecraft::Color::Aqua))?;
                    } else {
                        println!("no Minecraft nick matching Twitch nick {:?}", channel_name);
                    }
                } else {
                    println!("IRC channel name {:?} doesn't start with \"#\"", channel_name);
                }
            }
            Event::Error(e) => { return Err(e.into()); }
            event => { return Err(Error::UnexpectedTwitchEvent(event)); }
        }
    }
    Err(Error::TwitchClientTerminated)
}