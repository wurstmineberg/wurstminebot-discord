//! Writes info about voice channels to disk.

use std::{
    collections::BTreeMap,
    fs::File,
    io
};
use serde_json::{
    self,
    json
};
use serenity::model::user::User;
use typemap::Key;
use crate::base_path;

/// `typemap` key for the voice state data: A mapping of voice channel names to users.
pub struct VoiceStates;

impl Key for VoiceStates {
    type Value = BTreeMap<String, Vec<User>>;
}

/// Takes a mapping from voice channel names to users and dumps the output for the API.
pub fn dump_info(voice_states: &<VoiceStates as Key>::Value) -> io::Result<()> {
    let f = File::create(base_path().join("discord/voice-state.json"))?;
    serde_json::to_writer(f, &json!({
        "channels": voice_states.into_iter()
            .map(|(channel_name, members)| json!({
                "members": members.into_iter()
                    .map(|user| json!({
                        "discriminator": user.discriminator,
                        "snowflake": user.id,
                        "username": user.name
                    }))
                    .collect::<Vec<_>>(),
                "name": channel_name
            }))
            .collect::<Vec<_>>()
    }))?;
    Ok(())
}
