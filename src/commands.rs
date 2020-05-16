//! Implements of all of the bot's commands.

#![allow(missing_docs)]

use {
    rand::prelude::*,
    serenity::{
        framework::standard::{
            Args,
            CommandResult,
            macros::{
                command,
                group
            }
        },
        model::prelude::*,
        prelude::*,
        utils::MessageBuilder
    },
    crate::{
        Config,
        Database,
        emoji,
        parse,
        shut_down
    }
};

const GENERAL: ChannelId = ChannelId(88318761228054528);

#[command]
pub fn iam(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let mut sender = if let Some(sender) = msg.member(&ctx) {
        sender
    } else {
        //TODO get from `WURSTMINEBERG` guild instead of erroring
        msg.reply(ctx, "due to a technical limitation, this command currently doesn't work in DMs, sorry")?;
        return Ok(());
    };
    let mut cmd = args.message();
    let role = if let Some(role) = parse::eat_role_full(&mut cmd, msg.guild(&ctx)) {
        role
    } else {
        msg.reply(ctx, "no such role")?;
        return Ok(());
    };
    if !ctx.data.read().get::<Config>().expect("missing self-assignable roles list").wurstminebot.self_assignable_roles.contains(&role) {
        msg.reply(ctx, "this role is not self-assignable")?;
        return Ok(());
    }
    if sender.roles.contains(&role) {
        msg.reply(ctx, "you already have this role")?;
        return Ok(());
    }
    sender.add_role(&ctx, role)?;
    msg.reply(ctx, "role added")?;
    Ok(())
}

#[command]
pub fn iamn(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let mut sender = if let Some(sender) = msg.member(&ctx) {
        sender
    } else {
        //TODO get from `WURSTMINEBERG` guild instead of erroring
        msg.reply(ctx, "due to a technical limitation, this command currently doesn't work in DMs, sorry")?;
        return Ok(());
    };
    let mut cmd = args.message();
    let role = if let Some(role) = parse::eat_role_full(&mut cmd, msg.guild(&ctx)) {
        role
    } else {
        msg.reply(ctx, "no such role")?;
        return Ok(());
    };
    if !ctx.data.read().get::<Config>().expect("missing self-assignable roles list").wurstminebot.self_assignable_roles.contains(&role) {
        msg.reply(ctx, "this role is not self-assignable")?;
        return Ok(());
    }
    if !sender.roles.contains(&role) {
        msg.reply(ctx, "you already don't have this role")?;
        return Ok(());
    }
    sender.remove_role(&ctx, role)?;
    msg.reply(ctx, "role removed")?;
    Ok(())
}

#[command]
pub fn ping(ctx: &mut Context, msg: &Message, _: Args) -> CommandResult {
    let mut rng = thread_rng();
    let pingception = format!("BWO{}{}G", "R".repeat(rng.gen_range(3, 20)), "N".repeat(rng.gen_range(1, 5)));
    msg.reply(ctx, if rng.gen_bool(0.001) { &pingception } else { "pong" })?;
    Ok(())
}

#[command]
pub fn poll(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
    let mut emoji_iter = emoji::Iter::new(msg.content.to_owned())?.peekable();
    if emoji_iter.peek().is_some() {
        for emoji in emoji_iter {
            msg.react(&ctx, emoji)?;
        }
    } else if let Ok(num_reactions) = args.single::<u8>() {
        for i in 0..num_reactions.min(26) {
            msg.react(&ctx, emoji::nth_letter(i))?;
        }
    } else {
        msg.react(&ctx, "👍")?;
        msg.react(&ctx, "👎")?;
    }
    Ok(())
}

#[command]
#[owners_only]
fn quit(ctx: &mut Context, _: &Message, _: Args) -> CommandResult {
    shut_down(&ctx);
    Ok(())
}

#[command]
fn veto(ctx: &mut Context, _: &Message, args: Args) -> CommandResult {
    let data = ctx.data.read();
    let conn = data.get::<Database>().expect("missing database connection").lock();
    let mut cmd = args.message();
    let mut builder = MessageBuilder::default();
    builder.push("invite for ");
    match parse::eat_person(&mut cmd, &conn)? {
        Some(person) => { builder.mention(&person); } //TODO make sure remaining command is empty (or only whitespace), validate veto period, kick person from guild and remove from whitelist
        None => { builder.push_mono_safe(cmd); }
    }
    builder.push(" has been vetoed");
    GENERAL.say(&ctx, builder)?;
    Ok(())
}

#[group]
#[commands(
    iam,
    iamn,
    ping,
    poll,
    quit,
    veto
)]
struct Main;

pub use self::MAIN_GROUP as GROUP;
