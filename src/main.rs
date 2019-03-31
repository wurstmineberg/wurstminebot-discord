#![warn(trivial_casts)]
#![deny(unused)]
#![deny(rust_2018_idioms)] // this badly-named lint actually produces errors when Rust 2015 idioms are used
#![forbid(unused_import_braces)]

use std::{
    collections::{
        BTreeMap,
        HashMap,
        HashSet
    },
    env,
    io::{
        BufReader,
        prelude::*
    },
    net::TcpListener,
    process::{
        Command,
        Stdio
    },
    sync::Arc,
    thread,
    time::Duration
};
use chrono::prelude::*;
use diesel::prelude::*;
use serenity::{
    framework::standard::{
        StandardFramework,
        help_commands
    },
    model::prelude::*,
    prelude::*
};
use typemap::Key;
use wurstminebot::{
    Config,
    Database,
    Error,
    OtherError,
    ShardManagerContainer,
    WURSTMINEBERG,
    commands,
    people::Person,
    shut_down,
    voice::{
        self,
        VoiceStates
    }
};

#[derive(Default)]
struct Handler(Arc<Mutex<Option<Context>>>);

impl EventHandler for Handler {
    fn ready(&self, ctx: Context, ready: Ready) {
        *self.0.lock() = Some(ctx.clone());
        let guilds = ready.user.guilds().expect("failed to get guilds");
        if guilds.is_empty() {
            println!("[!!!!] No guilds found, use following URL to invite the bot:");
            println!("[ ** ] {}", ready.user.invite_url(Permissions::all()).expect("failed to generate invite URL"));
            shut_down(&ctx);
        } else if guilds.len() > 1 {
            println!("[!!!!] Multiple guilds found");
            shut_down(&ctx);
        }
    }

    fn guild_ban_addition(&self, ctx: Context, _: GuildId, user: User) {
        let data = ctx.data.lock();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::remove_discord_data(&conn, user).expect("failed to remove Discord data for banned user");
    }

    fn guild_ban_removal(&self, ctx: Context, guild_id: GuildId, user: User) {
        let data = ctx.data.lock();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::update_discord_data(&conn, &guild_id.member(user).expect("failed to get unbanned guild member")).expect("failed to update Discord data for unbanned user");
    }

    fn guild_create(&self, ctx: Context, guild: Guild, _: bool) {
        let mut chan_map = <VoiceStates as Key>::Value::default();
        for (user_id, voice_state) in guild.voice_states {
            if let Some(channel_id) = voice_state.channel_id {
                let user = user_id.to_user().expect("failed to get user info");
                let users = chan_map.entry(channel_id.name().expect("failed to get channel name"))
                    .or_insert_with(Vec::default);
                match users.binary_search_by_key(&(user.name.clone(), user.discriminator), |user| (user.name.clone(), user.discriminator)) {
                    Ok(idx) => { users[idx] = user; }
                    Err(idx) => { users.insert(idx, user); }
                }
            }
        }
        let mut data = ctx.data.lock();
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
        let data = ctx.data.lock();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::update_discord_data(&conn, &member).expect("failed to add Discord data for new guild member");
    }

    fn guild_member_removal(&self, ctx: Context, _: GuildId, user: User, _: Option<Member>) {
        let data = ctx.data.lock();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::remove_discord_data(&conn, user).expect("failed to remove Discord data for removed guild member");
    }

    fn guild_member_update(&self, ctx: Context, _: Option<Member>, member: Member) {
        let data = ctx.data.lock();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        Person::update_discord_data(&conn, &member).expect("failed to reflect guild member info update in database");
    }

    fn guild_members_chunk(&self, ctx: Context, _: GuildId, members: HashMap<UserId, Member>) {
        let data = ctx.data.lock();
        let conn = data.get::<Database>().expect("missing database connection").lock();
        for member in members.values() {
            Person::update_discord_data(&conn, member).expect("failed to update data for chunk of guild members in database");
        }
    }

    fn voice_state_update(&self, ctx: Context, _: Option<GuildId>, voice_state: VoiceState) {
        let user = voice_state.user_id.to_user().expect("failed to get user info");
        let mut data = ctx.data.lock();
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
        if let Some(channel_id) = voice_state.channel_id {
            let users = chan_map.entry(channel_id.name().expect("failed to get channel name"))
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
    for stream in TcpListener::bind(wurstminebot::IPC_ADDR)?.incoming() {
        let stream = stream?;
        for line in BufReader::new(&stream).lines() {
            let args = shlex::split(&line?).ok_or(OtherError::Shlex)?;
            match &args[0][..] {
                "quit" => {
                    let ctx_guard = ctx_arc.lock();
                    let ctx = ctx_guard.as_ref().ok_or(OtherError::MissingContext)?;
                    shut_down(&ctx);
                    thread::sleep(Duration::from_secs(1)); // wait to make sure websockets can be closed cleanly
                    writeln!(&mut &stream, "shutdown complete")?;
                }
                "set-display-name" => {
                    let user = args[1].parse::<UserId>()?.to_user()?;
                    let new_display_name = &args[2];
                    WURSTMINEBERG.edit_member(&user, |e| e.nickname(if &user.name == new_display_name { "" } else { new_display_name }))?;
                    writeln!(&mut &stream, "display name set")?;
                }
                _ => { return Err(OtherError::UnknownCommand(args).into()); }
            }
        }
    }
    unreachable!();
}

fn notify_ipc_crash(e: Error) {
    let mut child = Command::new("ssmtp")
        .arg("root@wurstmineberg.de")
        .stdin(Stdio::piped())
        .spawn()
        .expect("failed to spawn ssmtp");
    {
        let stdin = child.stdin.as_mut().expect("failed to open ssmtp stdin");
        write!(
            stdin,
            "To: root@wurstmineberg.de\nFrom: {}@{}\nSubject: wurstminebot IPC thread crashed\n\nwurstminebot IPC thread crashed with the following error:\n{}\n",
            whoami::username(),
            whoami::hostname(),
            e
        ).expect("failed to write to ssmtp stdin");
    }
    child.wait().expect("failed to wait for ssmtp subprocess"); //TODO check exit status
}

fn main() -> Result<(), Error> {
    let mut args = env::args().peekable();
    let _ = args.next(); // ignore executable name
    if args.peek().is_some() {
        println!("{}", wurstminebot::send_ipc_command(args)?);
    } else {
        // read config
        let config = Config::new()?;
        let handler = Handler::default();
        let ctx_arc = handler.0.clone();
        let mut client = Client::new(config.token(), handler)?;
        let owners = {
            let mut owners = HashSet::default();
            owners.insert(serenity::http::get_current_application_info()?.owner.id);
            owners
        };
        {
            let mut data = client.data.lock();
            data.insert::<Database>(Mutex::new(PgConnection::establish("postgres:///wurstmineberg")?));
            data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
            data.insert::<VoiceStates>(BTreeMap::default());
        }
        client.with_framework(StandardFramework::new()
            .configure(|c| c
                .allow_whitespace(true) // allow ! command
                .case_insensitivity(true) // allow !Command
                .no_dm_prefix(true) // allow /msg @wurstminebot command (also allows “did not understand DM” error messages to work)
                .on_mention(true) // allow @wurstminebot command
                .owners(owners)
                .prefix("!") // allow !command
            )
            .after(|_, msg, command_name, result| {
                if let Err(why) = result {
                    println!("{}: Command '{}' returned error {:?}", Utc::now().format("%Y-%m-%d %H:%M:%S"), command_name, why);
                    let _ = msg.reply(&format!("an error occurred while handling your command: {:?}", why)); //TODO notify an admin if this errors
                }
            })
            .unrecognised_command(|_, msg, _| {
                if msg.is_private() {
                    // reply when command isn't recognized
                    msg.reply("sorry, I don't understand that message").expect("failed to reply to unrecognized DM");
                }
            })
            .help(help_commands::with_embeds)
            .cmd("ping", commands::ping)
            .cmd("poll", commands::poll)
            .cmd("quit", commands::Quit)
            .cmd("veto", commands::veto)
        );
        // listen for IPC commands
        {
            thread::Builder::new().name("wurstminebot IPC".into()).spawn(move || {
                if let Err(e) = listen_ipc(ctx_arc) { //TODO remove `if` after changing from `()` to `!`
                    eprintln!("{}", e);
                    notify_ipc_crash(e);
                }
            })?;
        }
        // connect to Discord
        client.start_autosharded()?;
        thread::sleep(Duration::from_secs(1)); // wait to make sure websockets can be closed cleanly
    }
    Ok(())
}
