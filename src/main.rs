#![warn(trivial_casts)]
#![deny(unused)]
#![deny(rust_2018_idioms)] // this badly-named lint actually produces errors when Rust 2015 idioms are used
#![forbid(unused_import_braces)]

use std::{
    collections::BTreeMap,
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
use serenity::{
    model::{
        gateway::Ready,
        id::GuildId,
        permissions::Permissions,
        voice::VoiceState
    },
    prelude::*
};
use wurstminebot::{
    self,
    Config,
    Error,
    OtherError,
    ShardManagerContainer,
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
        {
            let mut data = client.data.lock();
            data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
            data.insert::<VoiceStates>(BTreeMap::default());
        }
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
