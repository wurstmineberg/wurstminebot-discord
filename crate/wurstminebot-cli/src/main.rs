#![deny(rust_2018_idioms, unused, unused_crate_dependencies, unused_import_braces, unused_lifetimes, unused_qualifications, warnings)]
#![forbid(unsafe_code)]

use {
    std::{
        collections::BTreeMap,
        env,
        iter,
        sync::Arc,
        time::{
            Duration,
            Instant,
        },
    },
    chrono::prelude::*,
    diesel::prelude::*,
    minecraft::chat::Chat,
    serenity::{
        async_trait,
        client::bridge::gateway::GatewayIntents,
        framework::standard::StandardFramework,
        http::Http,
        model::prelude::*,
        prelude::*,
    },
    serenity_utils::{
        RwFuture,
        ShardManagerContainer,
    },
    systemd_minecraft::World,
    tokio::time::sleep,
    wurstminebot::{
        Database,
        Error,
        commands,
        config::Config,
        log,
        minecraft::tellraw,
        people::Person,
        twitch,
        voice::{
            self,
            VoiceStates,
        },
    },
};

struct Handler(Arc<Mutex<Option<tokio::sync::oneshot::Sender<Context>>>>);

impl Handler {
    fn new() -> (Handler, RwFuture<Context>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (Handler(Arc::new(Mutex::new(Some(tx)))), RwFuture::new(async move { rx.await.expect("failed to store handler context") }))
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("Ready");
        if let Some(tx) = self.0.lock().await.take() {
            if let Err(_) = tx.send(ctx.clone()) {
                panic!("failed to send context")
            }
        }
        let guilds = ready.user.guilds(&ctx).await.expect("failed to get guilds");
        if guilds.is_empty() {
            println!("[!!!!] No guilds found, use following URL to invite the bot:");
            println!("[ ** ] {}", ready.user.invite_url(&ctx, Permissions::all()).await.expect("failed to generate invite URL"));
            serenity_utils::shut_down(&ctx).await;
        } else if guilds.len() > 1 {
            println!("[!!!!] Multiple guilds found");
            serenity_utils::shut_down(&ctx).await;
        }
    }

    async fn guild_ban_addition(&self, ctx: Context, guild_id: GuildId, user: User) {
        println!("User {} was banned from {}", user.name, guild_id);
        let data = ctx.data.read().await;
        let conn = data.get::<Database>().expect("missing database connection").lock().await;
        Person::remove_discord_data(&conn, user).expect("failed to remove Discord data for banned user");
    }

    async fn guild_ban_removal(&self, ctx: Context, guild_id: GuildId, user: User) {
        println!("User {} was unbanned from {}", user.name, guild_id);
        let member = &guild_id.member(&ctx, user).await.expect("failed to get unbanned guild member");
        let data = ctx.data.read().await;
        let conn = data.get::<Database>().expect("missing database connection").lock().await;
        Person::update_discord_data(&conn, member).expect("failed to update Discord data for unbanned user");
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, _: bool) {
        println!("Connected to {}, {} members total, {} members in list", guild.name, guild.member_count, guild.members.len());
        let mut chan_map = <VoiceStates as TypeMapKey>::Value::default();
        for (user_id, voice_state) in guild.voice_states {
            if let Some(channel_id) = voice_state.channel_id {
                let user = user_id.to_user(&ctx).await.expect("failed to get user info");
                let users = chan_map.entry(channel_id.name(&ctx).await.expect("failed to get channel name"))
                    .or_insert_with(Vec::default);
                match users.binary_search_by_key(&(user.name.clone(), user.discriminator), |user| (user.name.clone(), user.discriminator)) {
                    Ok(idx) => { users[idx] = user; }
                    Err(idx) => { users.insert(idx, user); }
                }
            }
        }
        let mut data = ctx.data.write().await;
        {
            let conn = data.get::<Database>().expect("missing database connection").lock().await;
            for member in guild.members.values() {
                Person::update_discord_data(&conn, member).expect("failed to update Discord data on guild_create");
            }
        }
        data.insert::<VoiceStates>(chan_map);
        let chan_map = data.get::<VoiceStates>().expect("missing voice states map");
        voice::dump_info(chan_map).expect("failed to update voice info");
    }

    async fn guild_member_addition(&self, ctx: Context, guild_id: GuildId, member: Member) {
        println!("User {} joined {}", member.user.name, guild_id);
        let data = ctx.data.read().await;
        let conn = data.get::<Database>().expect("missing database connection").lock().await;
        Person::update_discord_data(&conn, &member).expect("failed to add Discord data for new guild member");
    }

    async fn guild_member_removal(&self, ctx: Context, guild_id: GuildId, user: User, _: Option<Member>) {
        println!("User {} left {}", user.name, guild_id);
        let data = ctx.data.read().await;
        let conn = data.get::<Database>().expect("missing database connection").lock().await;
        Person::remove_discord_data(&conn, user).expect("failed to remove Discord data for removed guild member");
    }

    async fn guild_member_update(&self, ctx: Context, _: Option<Member>, member: Member) {
        println!("Member data for {} updated", member.user.name);
        let data = ctx.data.read().await;
        let conn = data.get::<Database>().expect("missing database connection").lock().await;
        Person::update_discord_data(&conn, &member).expect("failed to reflect guild member info update in database");
    }

    async fn guild_members_chunk(&self, ctx: Context, chunk: GuildMembersChunkEvent) {
        println!("Received chunk of members for guild {}", chunk.guild_id);
        let data = ctx.data.read().await;
        let conn = data.get::<Database>().expect("missing database connection").lock().await;
        for member in chunk.members.values() {
            Person::update_discord_data(&conn, member).expect("failed to update data for chunk of guild members in database");
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot { return; } // ignore bots to prevent message loops
        if let Some((world_name, _)) = ctx.data.read().await.get::<Config>().expect("missing config").wurstminebot.world_channels.iter().find(|(_, &chan_id)| chan_id == msg.channel_id) {
            let mut chat = Chat::from(format!(
                "[Discord:#{}] ",
                if let Some(Channel::Guild(chan)) = msg.channel(&ctx).await { chan.name.clone() } else { format!("?") },
            ));
            chat.color(minecraft::chat::Color::Aqua);
            chat.add_extra({
                let mut extra = Chat::from(format!("<{}>", msg.member.as_ref().and_then(|member| member.nick.as_ref()).unwrap_or(&msg.author.name)));
                extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(msg.author.tag()))));
                extra
            });
            chat.add_extra(" ");
            chat.add_extra(Chat::from(msg.content)); //TODO format mentions and emoji
            for attachment in msg.attachments {
                chat.add_extra(" ");
                chat.add_extra({
                    let mut extra = Chat::from(format!("[{}]", attachment.filename));
                    extra.color(minecraft::chat::Color::Blue);
                    extra.underlined();
                    extra.on_click(minecraft::chat::ClickEvent::OpenUrl(attachment.url.clone()));
                    extra.on_hover(minecraft::chat::HoverEvent::ShowText(Box::new(Chat::from(attachment.url))));
                    extra
                });
            }
            tellraw(&World::new(world_name), "@a", &chat).await.expect("chatsync failed");
        };
    }

    async fn voice_state_update(&self, ctx: Context, guild_id: Option<GuildId>, _old: Option<VoiceState>, new: VoiceState) {
        println!("Voice states in guild {:?} updated", guild_id);
        let user = new.user_id.to_user(&ctx).await.expect("failed to get user info");
        let mut data = ctx.data.write().await;
        let chan_map = data.get_mut::<VoiceStates>().expect("missing voice states map");
        let mut empty_channels = Vec::default();
        for (channel_name, users) in chan_map.iter_mut() {
            users.retain(|iter_user| iter_user.id != user.id);
            if users.is_empty() {
                empty_channels.push(channel_name.to_owned());
            }
        }
        for channel_name in empty_channels {
            chan_map.remove(&channel_name);
        }
        if let Some(channel_id) = new.channel_id {
            let users = chan_map.entry(channel_id.name(&ctx).await.expect("failed to get channel name"))
                .or_insert_with(Vec::default);
            match users.binary_search_by_key(&(user.name.clone(), user.discriminator), |user| (user.name.clone(), user.discriminator)) {
                Ok(idx) => { users[idx] = user; }
                Err(idx) => { users.insert(idx, user); }
            }
        }
        voice::dump_info(chan_map).expect("failed to update voice info");
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut args = env::args().peekable();
    let _ = args.next(); // ignore executable name
    if args.peek().is_some() {
        println!("{}", wurstminebot::ipc::send(args)?);
    } else {
        // read config
        let config = Config::new().await?;
        let (handler, rx) = Handler::new();
        let ctx_fut_ipc = rx.clone();
        let ctx_fut_log = rx.clone();
        let ctx_fut_twitch = rx;
        let owners = iter::once(Http::new_with_token(&config.wurstminebot.bot_token).get_current_application_info().await?.owner.id).collect();
        let mut client = Client::builder(&config.wurstminebot.bot_token)
            .event_handler(handler)
            .intents(
                GatewayIntents::DIRECT_MESSAGES
                | GatewayIntents::DIRECT_MESSAGE_REACTIONS
                | GatewayIntents::GUILDS
                | GatewayIntents::GUILD_PRESENCES // required for guild member data in guild_create
                | GatewayIntents::GUILD_MEMBERS
                | GatewayIntents::GUILD_BANS
                | GatewayIntents::GUILD_VOICE_STATES
                | GatewayIntents::GUILD_MESSAGES
            )
            .framework(StandardFramework::new()
                .configure(|c| c
                    .with_whitespace(true) // allow ! command
                    .case_insensitivity(true) // allow !Command
                    .no_dm_prefix(true) // allow /msg @wurstminebot command (also allows “did not understand DM” error messages to work)
                    .on_mention(Some(UserId(388416898825584640))) // allow @wurstminebot command
                    .owners(owners)
                    .prefix("!") // allow !command
                )
                .after(|ctx, msg, command_name, result| Box::pin(async move {
                    if let Err(why) = result {
                        println!("{}: Command '{}' returned error {:?}", Utc::now().format("%Y-%m-%d %H:%M:%S"), command_name, why);
                        let _ = msg.reply(ctx, &format!("an error occurred while handling your command: {:?}", why)).await; //TODO notify an admin if this errors
                    }
                }))
                .unrecognised_command(|ctx, msg, _| Box::pin(async move {
                    if msg.author.bot { return } // ignore bots to prevent message loops
                    if msg.is_private() {
                        // reply when command isn't recognized
                        msg.reply(ctx, "sorry, I don't understand that message").await.expect("failed to reply to unrecognized DM");
                    }
                }))
                .help(&commands::HELP_COMMAND)
                .group(&commands::GROUP)
            )
            .type_map_insert::<Config>(config)
            .type_map_insert::<Database>(Mutex::new(PgConnection::establish("postgres:///wurstmineberg")?))
            .type_map_insert::<VoiceStates>(BTreeMap::default())
            .await?;
        client.data.write().await.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
        // listen for IPC commands
        tokio::spawn(async move {
            match wurstminebot::ipc::listen(ctx_fut_ipc.clone(), &|ctx, thread_kind, e| wurstminebot::notify_thread_crash(ctx, thread_kind, e, None)).await {
                Ok(never) => match never {},
                Err(e) => {
                    eprintln!("{}", e);
                    wurstminebot::notify_thread_crash(ctx_fut_ipc.clone(), format!("IPC"), e, None).await;
                }
            }
        });
        // follow the Minecraft log
        tokio::spawn(async move {
            if let Err(e) = log::handle(ctx_fut_log.clone()).await {
                eprintln!("{}", e);
                wurstminebot::notify_thread_crash(ctx_fut_log.clone(), format!("log"), e, None).await;
            }
        });
        // listen for Twitch chat messages
        tokio::spawn(async move {
            let mut last_crash = Instant::now();
            let mut wait_time = Duration::from_secs(1);
            loop {
                let e = match twitch::listen_chat(ctx_fut_twitch.clone()).await {
                    Ok(never) => match never {},
                    Err(e) => e,
                };
                if last_crash.elapsed() >= Duration::from_secs(60 * 60 * 24) {
                    wait_time = Duration::from_secs(1); // reset wait time after no crash for a day
                } else {
                    wait_time *= 2; // exponential backoff
                }
                eprintln!("{} ({:?})", e, e);
                //wurstminebot::notify_thread_crash(ctx_fut_twitch.clone(), format!("Twitch"), e, Some(wait_time)).await; //TODO uncomment after https://github.com/museun/twitchchat/issues/237 is fixed
                sleep(wait_time).await; // wait before attempting to reconnect
                last_crash = Instant::now();
            }
        });
        // connect to Discord
        client.start_autosharded().await?;
        sleep(Duration::from_secs(1)).await; // wait to make sure websockets can be closed cleanly
    }
    Ok(())
}
