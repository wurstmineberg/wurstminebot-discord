#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        collections::{
            BTreeMap,
            HashMap
        },
        env,
        io::{
            BufReader,
            prelude::*
        },
        iter,
        net::TcpListener,
        process::{
            Command,
            Stdio
        },
        sync::Arc,
        thread,
        time::Duration
    },
    chrono::prelude::*,
    diesel::prelude::*,
    serenity::{
        framework::standard::StandardFramework,
        model::prelude::*,
        prelude::*
    },
    systemd_minecraft::World,
    typemap::Key,
    wurstminebot::{
        Config,
        Database,
        Error,
        IntoResult,
        ShardManagerContainer,
        WURSTMINEBERG,
        commands,
        minecraft::{
            self,
            Chat
        },
        people::Person,
        shut_down,
        twitch,
        voice::{
            self,
            VoiceStates
        }
    }
};

const DEV: ChannelId = ChannelId(506905544901001228);

#[derive(Default)]
struct Handler(Arc<Mutex<Option<Context>>>);

impl EventHandler for Handler {
    fn ready(&self, ctx: Context, ready: Ready) {
        *self.0.lock() = Some(ctx.clone());
        let guilds = ready.user.guilds(&ctx).expect("failed to get guilds");
        if guilds.is_empty() {
            println!("[!!!!] No guilds found, use following URL to invite the bot:");
            println!("[ ** ] {}", ready.user.invite_url(&ctx, Permissions::all()).expect("failed to generate invite URL"));
            shut_down(&ctx);
        } else if guilds.len() > 1 {
            println!("[!!!!] Multiple guilds found");
            shut_down(&ctx);
        }
    }

    fn guild_ban_addition(&self, ctx: Context, _: GuildId, user: User) {
        let data = ctx.data.read();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::remove_discord_data(&conn, user).expect("failed to remove Discord data for banned user");
    }

    fn guild_ban_removal(&self, ctx: Context, guild_id: GuildId, user: User) {
        let data = ctx.data.read();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::update_discord_data(&conn, &guild_id.member(&ctx, user).expect("failed to get unbanned guild member")).expect("failed to update Discord data for unbanned user");
    }

    fn guild_create(&self, ctx: Context, guild: Guild, _: bool) {
        let mut chan_map = <VoiceStates as Key>::Value::default();
        for (user_id, voice_state) in guild.voice_states {
            if let Some(channel_id) = voice_state.channel_id {
                let user = user_id.to_user(&ctx).expect("failed to get user info");
                let users = chan_map.entry(channel_id.name(&ctx).expect("failed to get channel name"))
                    .or_insert_with(Vec::default);
                match users.binary_search_by_key(&(user.name.clone(), user.discriminator), |user| (user.name.clone(), user.discriminator)) {
                    Ok(idx) => { users[idx] = user; }
                    Err(idx) => { users.insert(idx, user); }
                }
            }
        }
        let mut data = ctx.data.write();
        {
            let conn = data.get::<Database>().expect("missing database connection").lock();
            for member in guild.members.values() {
                Person::update_discord_data(&conn, member).expect("failed to update Discord data on guild_create");
            }
        }
        data.insert::<VoiceStates>(chan_map);
        let chan_map = data.get::<VoiceStates>().expect("missing voice states map");
        voice::dump_info(chan_map).expect("failed to update voice info");
    }

    fn guild_member_addition(&self, ctx: Context, _: GuildId, member: Member) {
        let data = ctx.data.read();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::update_discord_data(&conn, &member).expect("failed to add Discord data for new guild member");
    }

    fn guild_member_removal(&self, ctx: Context, _: GuildId, user: User, _: Option<Member>) {
        let data = ctx.data.read();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::remove_discord_data(&conn, user).expect("failed to remove Discord data for removed guild member");
    }

    fn guild_member_update(&self, ctx: Context, _: Option<Member>, member: Member) {
        let data = ctx.data.read();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::update_discord_data(&conn, &member).expect("failed to reflect guild member info update in database");
    }

    fn guild_members_chunk(&self, ctx: Context, _: GuildId, members: HashMap<UserId, Member>) {
        let data = ctx.data.read();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        for member in members.values() {
            Person::update_discord_data(&conn, member).expect("failed to update data for chunk of guild members in database");
        }
    }

    fn message(&self, ctx: Context, msg: Message) {
        if let Some((world_name, _)) = ctx.data.read().get::<Config>().expect("missing config").wurstminebot.world_channels.iter().find(|(_, &chan_id)| chan_id == msg.channel_id) {
            minecraft::tellraw(&World::new(world_name), "@a", Chat::from(format!(
                "[Discord:#{}] <{}> {}",
                if let Some(Channel::Guild(chan)) = msg.channel(&ctx) { chan.read().name.clone() } else { format!("?") },
                msg.author.name, //TODO replace with nickname, include discriminator if nickname is ambiguous
                msg.content
            )).color(minecraft::Color::Aqua)).expect("chatsync failed");
        }
    }

    fn voice_state_update(&self, ctx: Context, _: Option<GuildId>, _old: Option<VoiceState>, new: VoiceState) {
        let user = new.user_id.to_user(&ctx).expect("failed to get user info");
        let mut data = ctx.data.write();
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
            let users = chan_map.entry(channel_id.name(&ctx).expect("failed to get channel name"))
                .or_insert_with(Vec::default);
            match users.binary_search_by_key(&(user.name.clone(), user.discriminator), |user| (user.name.clone(), user.discriminator)) {
                Ok(idx) => { users[idx] = user; }
                Err(idx) => { users.insert(idx, user); }
            }
        }
        voice::dump_info(chan_map).expect("failed to update voice info");
    }
}

fn listen_ipc(ctx_arc: Arc<Mutex<Option<Context>>>) -> Result<(), Error> { //TODO change return type to Result<!, Error>
    for stream in TcpListener::bind(wurstminebot::IPC_ADDR).annotate("failed to start listening on IPC port")?.incoming() {
        let stream = stream.annotate("failed to initialize IPC connection")?;
        for line in BufReader::new(&stream).lines() {
            let args = shlex::split(&line.annotate("failed to read IPC command")?).ok_or(Error::Shlex)?;
            match &args[0][..] {
                "quit" => {
                    let ctx_guard = ctx_arc.lock();
                    let ctx = ctx_guard.as_ref().ok_or(Error::MissingContext)?;
                    shut_down(&ctx);
                    thread::sleep(Duration::from_secs(1)); // wait to make sure websockets can be closed cleanly
                    writeln!(&mut &stream, "shutdown complete").annotate("failed to send quit confirmation")?;
                }
                "set-display-name" => {
                    let ctx_guard = ctx_arc.lock();
                    let ctx = ctx_guard.as_ref().ok_or(Error::MissingContext)?;
                    let user = args[1].parse::<UserId>().annotate("failed to parse user for set-display-name")?.to_user(ctx).annotate("failed to get user for set-display-name")?;
                    let new_display_name = &args[2];
                    match WURSTMINEBERG.edit_member(ctx, &user, |e| e.nickname(if &user.name == new_display_name { "" } else { new_display_name })) {
                        Ok(()) => {
                            writeln!(&mut &stream, "display name set").annotate("failed to send set-display-name confirmation")?;
                        }
                        Err(serenity::Error::Http(e)) => if let HttpError::UnsuccessfulRequest(response) = *e {
                            writeln!(&mut &stream, "failed to set display name: {:?}", response)?;
                        } else {
                            //TODO use box patterns to eliminate this branch and use the next match arm instead
                            return Err(serenity::Error::Http(e)).annotate("failed to edit member");
                        },
                        Err(e) => { return Err(e).annotate("failed to edit member"); }
                    }
                }
                _ => { return Err(Error::UnknownCommand(args)); }
            }
        }
    }
    unreachable!();
}

fn notify_thread_crash(ctx: &Option<Context>, thread_kind: &str, e: Error) {
    if ctx.as_ref().and_then(|ctx| DEV.say(ctx, format!("{} thread crashed: {} (`{:?}`)", thread_kind, e, e)).ok()).is_none() {
        let mut child = Command::new("mail")
            .arg("-s")
            .arg(format!("wurstminebot {} thread crashed", thread_kind))
            .arg("root@wurstmineberg.de")
            .stdin(Stdio::piped())
            .spawn()
            .expect("failed to spawn mail");
        {
            let stdin = child.stdin.as_mut().expect("failed to open mail stdin");
            write!(stdin, "wurstminebot {} thread crashed with the following error:\n{}\n{:?}\n", thread_kind, e, e).expect("failed to write to mail stdin");
        }
        child.wait().expect("failed to wait for mail subprocess"); //TODO check exit status
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut args = env::args().peekable();
    let _ = args.next(); // ignore executable name
    if args.peek().is_some() {
        println!("{}", wurstminebot::send_ipc_command(args)?);
    } else {
        // read config
        let config = Config::new()?;
        let handler = Handler::default();
        let ctx_arc_ipc = handler.0.clone();
        let ctx_arc_twitch = handler.0.clone();
        let mut client = Client::new(config.token(), handler)?;
        let owners = iter::once(client.cache_and_http.http.get_current_application_info()?.owner.id).collect();
        {
            let mut data = client.data.write();
            data.insert::<Config>(config);
            data.insert::<Database>(Mutex::new(PgConnection::establish("postgres:///wurstmineberg")?));
            data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
            data.insert::<VoiceStates>(BTreeMap::default());
        }
        client.with_framework(StandardFramework::new()
            .configure(|c| c
                .with_whitespace(true) // allow ! command
                .case_insensitivity(true) // allow !Command
                .no_dm_prefix(true) // allow /msg @wurstminebot command (also allows “did not understand DM” error messages to work)
                .on_mention(Some(UserId(388416898825584640))) // allow @wurstminebot command
                .owners(owners)
                .prefix("!") // allow !command
            )
            .after(|ctx, msg, command_name, result| {
                if let Err(why) = result {
                    println!("{}: Command '{}' returned error {:?}", Utc::now().format("%Y-%m-%d %H:%M:%S"), command_name, why);
                    let _ = msg.reply(ctx, &format!("an error occurred while handling your command: {:?}", why)); //TODO notify an admin if this errors
                }
            })
            .unrecognised_command(|ctx, msg, _| {
                if msg.author.bot { return; } // ignore bots to prevent message loops
                if msg.is_private() {
                    // reply when command isn't recognized
                    msg.reply(ctx, "sorry, I don't understand that message").expect("failed to reply to unrecognized DM");
                }
            })
            //.help(help_commands::with_embeds) //TODO fix help?
            .group(&commands::GROUP)
        );
        // listen for IPC commands
        //TODO rewrite using tokio
        {
            thread::Builder::new().name(format!("wurstminebot IPC")).spawn(move || {
                if let Err(e) = listen_ipc(ctx_arc_ipc.clone()) { //TODO remove `if` after changing from `()` to `!`
                    eprintln!("{}", e);
                    notify_thread_crash(&ctx_arc_ipc.lock(), "IPC", e);
                }
            })?;
        }
        // listen for Twitch chat messages
        {
            let data = client.data.read();
            let conn = data.get::<Database>().expect("missing database connection").lock();
            let everyone = Person::all(&conn)?;
            tokio::spawn(async move {
                if let Err(e) = twitch::listen_chat(World::default(), everyone).await {
                    eprintln!("{}", e);
                    notify_thread_crash(&ctx_arc_twitch.lock(), "Twitch", e);
                }
            });
        }
        // connect to Discord
        client.start_autosharded()?;
        thread::sleep(Duration::from_secs(1)); // wait to make sure websockets can be closed cleanly
    }
    Ok(())
}
