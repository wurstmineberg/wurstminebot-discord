#![warn(trivial_casts)]
#![deny(unused)]
#![deny(rust_2018_idioms)] // this badly-named lint actually produces errors when Rust 2015 idioms are used
#![forbid(unused_import_braces)]

use std::{
    collections::BTreeMap,
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
    Config,
    ShardManagerContainer,
    shut_down,
    voice::{
        self,
        VoiceStates
    }
};

struct Handler;

impl EventHandler for Handler {
    fn ready(&self, ctx: Context, ready: Ready) {
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

fn main() -> Result<(), wurstminebot::Error> {
    let config = Config::new()?;
    let mut client = Client::new(config.token(), Handler)?;
    {
        let mut data = client.data.lock();
        data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
        data.insert::<VoiceStates>(BTreeMap::default());
    }
    client.start_autosharded()?;
    thread::sleep(Duration::from_secs(1)); // wait to make sure websockets can be closed cleanly
    Ok(())
}
