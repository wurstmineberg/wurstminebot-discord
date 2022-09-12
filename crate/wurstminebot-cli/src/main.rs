#![deny(rust_2018_idioms, unused, unused_crate_dependencies, unused_import_braces, unused_lifetimes, unused_qualifications, warnings)]
#![forbid(unsafe_code)]

use {
    std::{
        future::Future,
        pin::Pin,
        time::{
            Duration,
            Instant,
        },
    },
    discord_message_parser::{
        MessagePart,
        TimestampStyle,
        serenity::MessageExt as _,
    },
    itertools::Itertools as _,
    minecraft::chat::Chat,
    serde_json::json,
    serenity::{
        model::prelude::*,
        prelude::*,
    },
    serenity_utils::{
        builder::ErrorNotifier,
        handler::{
            HandlerMethods as _,
            voice_state::VoiceStates,
        },
    },
    sqlx::postgres::{
        PgConnectOptions,
        PgPool,
    },
    systemd_minecraft::World,
    tokio::{
        fs,
        process::Command,
        time::sleep,
    },
    wurstminebot::{
        DEV,
        Database,
        Error,
        cal,
        commands::*,
        config::Config,
        http,
        minecraft::tellraw,
        twitch,
    },
};
#[cfg(unix)] use wurstminebot::log;

enum UserListExporter {}

impl serenity_utils::handler::user_list::ExporterMethods for UserListExporter {
    fn upsert<'a>(ctx: &'a Context, member: &'a Member) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>> {
        Box::pin(async move {
            let data = ctx.data.read().await;
            let pool = data.get::<Database>().expect("missing database connection");
            //TODO update display name in data column
            sqlx::query!("UPDATE people SET discorddata = $1 WHERE snowflake = $2", json!({
                "avatar": member.user.avatar_url(),
                "discriminator": member.user.discriminator,
                "joined": if let Some(ref join_date) = member.joined_at { join_date } else { return Err(Box::new(Error::MissingJoinDate) as Box<dyn std::error::Error + Send + Sync>) },
                "nick": &member.nick,
                "roles": &member.roles,
                "username": member.user.name,
            }), member.user.id.0 as i64)
                .execute(pool).await?;
            Ok(())
        })
    }

    fn replace_all<'a>(ctx: &'a Context, members: Vec<&'a Member>) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>> {
        Box::pin(async move {
            for member in members {
                Self::upsert(ctx, member).await?;
            }
            Ok(())
        })
    }

    fn remove<'a>(ctx: &'a Context, UserId(user_id): UserId, _: GuildId) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>> {
        Box::pin(async move {
            let data = ctx.data.read().await;
            let pool = data.get::<Database>().expect("missing database connection");
            sqlx::query!("UPDATE people SET discorddata = NULL WHERE snowflake = $1", user_id as i64)
                .execute(pool).await?;
            Ok(())
        })
    }
}

enum VoiceStateExporter {}

impl serenity_utils::handler::voice_state::ExporterMethods for VoiceStateExporter {
    fn dump_info<'a>(_: &'a Context, _: GuildId, VoiceStates(voice_states): &'a VoiceStates) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>> {
        Box::pin(async move {
            let buf = serde_json::to_vec_pretty(&json!({
                "channels": voice_states.into_iter()
                    .map(|(channel_id, (channel_name, members))| json!({
                        "members": members.into_iter()
                            .map(|user| json!({
                                "discriminator": user.discriminator,
                                "snowflake": user.id,
                                "username": user.name,
                            }))
                            .collect_vec(),
                        "name": channel_name,
                        "snowflake": channel_id,
                    }))
                    .collect_vec()
            }))?;
            fs::write(wurstminebot::base_path().join("discord/voice-state.json"), buf).await?;
            Ok(())
        })
    }
}

fn discord_to_minecraft<'a>(ctx: &'a Context, msg: &'a Message, chat: &'a mut Chat, part: MessagePart<'a>) -> Pin<Box<dyn Future<Output = serenity::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        match part {
            MessagePart::Empty => {}
            MessagePart::Nested(parts) => for part in parts {
                discord_to_minecraft(ctx, msg, chat, part).await?;
            },
            MessagePart::PlainText(text) => { chat.add_extra(text); }
            MessagePart::UserMention { user, nickname_mention: _ } => {
                let (tag, nickname) = if let Some(guild_id) = msg.guild_id {
                    let member = guild_id.member(ctx, user).await?;
                    (Some(member.user.tag()), member.nick)
                } else {
                    (None, None)
                };
                let (tag, nickname) = match (tag, nickname) {
                    (Some(tag), Some(nickname)) => (tag, nickname),
                    (tag, nickname) => {
                        let user = user.to_user(ctx).await?;
                        (tag.unwrap_or_else(|| user.tag()), nickname.unwrap_or(user.name))
                    }
                };
                let mut extra = Chat::from(format!("@{}", nickname));
                //TODO add mention to chat input on click? (blue + underline)
                extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(tag))));
                chat.add_extra(extra);
            }
            MessagePart::ChannelMention(channel) => {
                let extra = Chat::from(match channel.to_channel(ctx).await? {
                    Channel::Guild(channel) => format!("#{}", channel.name),
                    Channel::Private(dm) => dm.name(),
                    Channel::Category(category) => category.name,
                    _ => panic!("unexpected channel type"),
                });
                //TODO open channel in browser on click? (blue + underline)
                chat.add_extra(extra);
            }
            MessagePart::RoleMention(role) => {
                let mut extra = Chat::from(format!("<@&{}>", role));
                if let Some(guild_id) = msg.guild_id {
                    if let Some(role) = guild_id.roles(ctx).await?.get(&role) {
                        extra = Chat::from(format!("@{}", role.name));
                        //TODO add mention to chat input on click? (blue + underline)
                    }
                }
                chat.add_extra(extra);
            }
            MessagePart::UnicodeEmoji(text) => { chat.add_extra(text); } //TODO special handling for emoji where possible
            MessagePart::CustomEmoji(emoji) => {
                chat.add_extra(format!(":{}:", emoji.name));
            }
            MessagePart::Timestamp { timestamp, style } => {
                let mut extra = Chat::from(style.unwrap_or_default().fmt(timestamp)); //TODO convert to user timezone? (Would require replacing @a with individual commands)
                extra.underlined();
                if let Some(TimestampStyle::RelativeTime) = style {
                    extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string())))); //TODO show user timezone if converted
                } else {
                    extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from("UTC")))); //TODO show user timzeone if converted
                }
                chat.add_extra(extra);
            }
        }
        Ok(())
    })
}

#[serenity_utils::main(
    ipc = "wurstminebot::ipc",
    slash_commands(iam, iamn, ping, quit, update),
)]
async fn main() -> Result<serenity_utils::Builder, Error> {
    let config = Config::new().await?;
    Ok(serenity_utils::builder(388416898825584640, config.wurstminebot.bot_token.clone()).await?
        .error_notifier(ErrorNotifier::Channel(DEV))
        .on_ready(|ctx, ready| Box::pin(async move {
            if ready.user.guilds(ctx).await.expect("failed to get guilds").len() > 1 {
                println!("error: multiple guilds found (wurstminebot's code currently assumes that it's only in the Wurstmineberg guild)"); //TODO return as Err?
                serenity_utils::shut_down(&ctx).await;
            }
            Ok(())
        }))
        .on_message(true, |ctx, msg| Box::pin(async move {
            if msg.author.bot { return Ok(()) } // ignore bots to prevent message loops
            if let Some((world_name, _)) = ctx.data.read().await.get::<Config>().expect("missing config").wurstminebot.world_channels.iter().find(|(_, &chan_id)| chan_id == msg.channel_id) {
                if Command::new("systemctl").arg("is-active").arg(format!("minecraft@{world_name}.service")).status().await?.success() {
                    let mut chat = Chat::from(format!(
                        "[Discord:#{}",
                        if let Channel::Guild(chan) = msg.channel(&ctx).await? { chan.name.clone() } else { format!("?") },
                    ));
                    chat.color(minecraft::chat::Color::Aqua);
                    if let Some(ref in_reply_to) = msg.referenced_message {
                        chat.add_extra(", replying to ");
                        chat.add_extra({
                            let mut extra = Chat::from(in_reply_to.member.as_ref().and_then(|member| member.nick.as_deref()).unwrap_or(&in_reply_to.author.name));
                            extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(in_reply_to.author.tag()))));
                            extra
                        });
                    }
                    chat.add_extra("] ");
                    chat.add_extra({
                        let mut extra = Chat::from(format!("<{}>", msg.member.as_ref().and_then(|member| member.nick.as_ref()).unwrap_or(&msg.author.name)));
                        extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(msg.author.tag()))));
                        extra
                    });
                    chat.add_extra(" ");
                    discord_to_minecraft(&ctx, &msg, &mut chat, msg.parse()).await?;
                    for attachment in &msg.attachments {
                        chat.add_extra(" ");
                        chat.add_extra({
                            let mut extra = Chat::from(format!("[{}]", attachment.filename));
                            extra.color(minecraft::chat::Color::Blue);
                            extra.underlined();
                            extra.on_click(minecraft::chat::ClickEvent::OpenUrl(attachment.url.clone()));
                            extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(&*attachment.url))));
                            extra
                        });
                    }
                    match tellraw(&World::new(world_name), "@a", &chat).await {
                        Ok(_) => {}
                        Err(Error::Minecraft(systemd_minecraft::Error::Rcon(rcon::Error::CommandTooLong))) => {
                            let mut chat = Chat::from(format!(
                                "[Discord:#{}] long message from ",
                                if let Channel::Guild(chan) = msg.channel(&ctx).await? { chan.name.clone() } else { format!("?") },
                            ));
                            chat.color(minecraft::chat::Color::Aqua);
                            chat.add_extra({
                                let mut extra = Chat::from(msg.member.as_ref().and_then(|member| member.nick.as_deref()).unwrap_or(&msg.author.name));
                                extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(msg.author.tag()))));
                                extra
                            });
                            tellraw(&World::new(world_name), "@a", &chat).await?;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            Ok(())
        }))
        .event_handler(serenity_utils::handler::user_list_exporter::<UserListExporter>())
        .event_handler(serenity_utils::handler::voice_state_exporter::<VoiceStateExporter>())
        .message_commands(Some("!"), &GROUP) //TODO migrate to slash commands
        .data::<Config>(config)
        .data::<Database>(PgPool::connect_with(PgConnectOptions::default().database("wurstmineberg").application_name("wurstminebot")).await?)
        .task(|ctx_fut, notify_thread_crash| async move {
            if let Err(e) = cal::notifications(ctx_fut).await {
                eprintln!("{}", e);
                notify_thread_crash(format!("calendar notifications"), Box::new(e), None).await;
            }
        })
        .task(|ctx_fut, notify_thread_crash| async move {
            if let Err(e) = http::rocket(ctx_fut).launch().await {
                eprintln!("{}", e);
                notify_thread_crash(format!("HTTP server"), Box::new(e), None).await;
            }
        })
        .task(|#[cfg_attr(not(unix), allow(unused))] ctx_fut, #[cfg_attr(not(unix), allow(unused))] notify_thread_crash| async move {
            #[cfg(unix)] {
                // follow the Minecraft log
                if let Err(e) = log::handle(ctx_fut).await {
                    eprintln!("{}", e);
                    notify_thread_crash(format!("log"), Box::new(e), None).await;
                }
            }
            #[cfg(not(unix))] {
                eprintln!("warning: Minecraft log analysis is only supported on cfg(unix) because of https://github.com/lloydmeta/chase-rs/issues/6");
            }
        })
        .task(|ctx_fut, notify_thread_crash| async move {
            // listen for Twitch chat messages
            let mut last_crash = Instant::now();
            let mut wait_time = Duration::from_secs(1);
            loop {
                let e = match twitch::listen_chat(ctx_fut.clone()).await {
                    Ok(never) => match never {},
                    Err(e) => e,
                };
                if last_crash.elapsed() >= Duration::from_secs(60 * 60 * 24) {
                    wait_time = Duration::from_secs(1); // reset wait time after no crash for a day
                } else {
                    wait_time *= 2; // exponential backoff
                }
                eprintln!("{} ({:?})", e, e);
                if wait_time >= Duration::from_secs(2) { // only notify on multiple consecutive errors
                    notify_thread_crash(format!("Twitch"), Box::new(e), Some(wait_time)).await;
                }
                sleep(wait_time).await; // wait before attempting to reconnect
                last_crash = Instant::now();
            }
        })
    )
}
