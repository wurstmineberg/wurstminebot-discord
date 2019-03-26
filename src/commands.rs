//! Implements of all of the bot's commands.

#![allow(missing_docs)]

use std::sync::Arc;
use rand::prelude::*;
use serenity::{
    framework::standard::{
        Args,
        Command,
        CommandOptions,
        CommandError
    },
    model::prelude::*,
    prelude::*,
    utils::MessageBuilder
};
use crate::{
    Database,
    emoji,
    parse,
    shut_down
};

const GENERAL: ChannelId = ChannelId(88318761228054528);

pub fn ping(_: &mut Context, msg: &Message, _: Args) -> Result<(), CommandError> {
    let mut rng = thread_rng();
    let pingception = format!("BWO{}{}G", "R".repeat(rng.gen_range(3, 20)), "N".repeat(rng.gen_range(1, 5)));
    msg.reply(if rng.gen_bool(0.001) { &pingception } else { "pong" })?;
    Ok(())
}

pub fn poll(_: &mut Context, msg: &Message, mut args: Args) -> Result<(), CommandError> {
    let mut emoji_iter = emoji::Iter::new(msg.content.to_owned())?.peekable();
    if emoji_iter.peek().is_some() {
        for emoji in emoji_iter {
            msg.react(emoji)?;
        }
    } else if let Ok(num_reactions) = args.single::<u8>() {
        for i in 0..num_reactions.min(26) {
            msg.react(emoji::nth_letter(i))?;
        }
    } else {
        msg.react("👍")?;
        msg.react("👎")?;
    }
    Ok(())
}

pub struct Quit;

impl Command for Quit {
    fn execute(&self, ctx: &mut Context, _: &Message, _: Args) -> Result<(), CommandError> {
        shut_down(&ctx);
        Ok(())
    }

    fn options(&self) -> Arc<CommandOptions> {
        Arc::new(CommandOptions {
            owners_only: true,
            ..CommandOptions::default()
        })
    }
}

pub fn veto(ctx: &mut Context, _: &Message, args: Args) -> Result<(), CommandError> {
    let data = ctx.data.lock();
    let conn = data.get::<Database>().expect("missing database connection").lock();
    let mut cmd = args.full();
    let builder = MessageBuilder::default().push("invite for ");
    let builder = match parse::eat_person(&mut cmd, &conn)? {
        Some(person) => builder.mention(&person), //TODO make sure remaining command is empty (or only whitespace), validate veto period, kick person from guild and remove from whitelist
        None => builder.push_mono_safe(cmd)
    }.push(" has been vetoed");
    GENERAL.say(builder)?;
    Ok(())
}
