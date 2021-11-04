//! Implements of all of the bot's commands.

#![allow(missing_docs)]

use {
    std::collections::HashSet,
    rand::prelude::*,
    serenity::{
        framework::standard::{
            Args,
            CommandGroup,
            CommandResult,
            HelpOptions,
            help_commands,
            macros::{
                command,
                group,
                help,
            },
        },
        model::prelude::*,
        prelude::*,
        utils::MessageBuilder,
    },
    systemd_minecraft::{
        VersionSpec,
        World,
    },
    crate::{
        Database,
        GENERAL,
        config::Config,
        emoji,
        parse,
    },
};
pub use self::{
    HELP as HELP_COMMAND,
    MAIN_GROUP as GROUP,
};

#[help]
async fn help(ctx: &Context, msg: &Message, args: Args, help_options: &'static HelpOptions, groups: &[&'static CommandGroup], owners: HashSet<UserId>) -> CommandResult {
    let _ = help_commands::with_embeds(ctx, msg, args, help_options, groups, owners).await;
    Ok(())
}

#[command]
pub async fn iam(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let mut sender = if let Ok(sender) = msg.member(ctx).await {
        sender
    } else {
        //TODO get from `WURSTMINEBERG` guild instead of erroring
        msg.reply(ctx, "due to a technical limitation, this command currently doesn't work in DMs, sorry").await?;
        return Ok(());
    };
    let mut cmd = args.message();
    let role = if let Some(role) = parse::eat_role_full(&mut cmd, msg.guild(ctx).await).await {
        role
    } else {
        msg.reply(ctx, "no such role").await?;
        return Ok(());
    };
    if !ctx.data.read().await.get::<Config>().expect("missing self-assignable roles list").wurstminebot.self_assignable_roles.contains(&role) {
        msg.reply(ctx, "this role is not self-assignable").await?;
        return Ok(());
    }
    if sender.roles.contains(&role) {
        msg.reply(ctx, "you already have this role").await?;
        return Ok(());
    }
    sender.add_role(&ctx, role).await?;
    msg.reply(ctx, "role added").await?;
    Ok(())
}

#[command]
pub async fn iamn(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let mut sender = if let Ok(sender) = msg.member(&ctx).await {
        sender
    } else {
        //TODO get from `WURSTMINEBERG` guild instead of erroring
        msg.reply(ctx, "due to a technical limitation, this command currently doesn't work in DMs, sorry").await?;
        return Ok(());
    };
    let mut cmd = args.message();
    let role = if let Some(role) = parse::eat_role_full(&mut cmd, msg.guild(&ctx).await).await {
        role
    } else {
        msg.reply(ctx, "no such role").await?;
        return Ok(());
    };
    if !ctx.data.read().await.get::<Config>().expect("missing self-assignable roles list").wurstminebot.self_assignable_roles.contains(&role) {
        msg.reply(ctx, "this role is not self-assignable").await?;
        return Ok(());
    }
    if !sender.roles.contains(&role) {
        msg.reply(ctx, "you already don't have this role").await?;
        return Ok(());
    }
    sender.remove_role(&ctx, role).await?;
    msg.reply(ctx, "role removed").await?;
    Ok(())
}

#[command]
pub async fn ping(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    let reply = {
        let mut rng = thread_rng();
        if rng.gen_bool(0.01) { format!("BWO{}{}G", "R".repeat(rng.gen_range(3..20)), "N".repeat(rng.gen_range(1..5))) } else { format!("pong") }
    };
    msg.reply(ctx, reply).await?;
    Ok(())
}

#[command]
pub async fn poll(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let mut emoji_iter = emoji::Iter::new(msg).peekable();
    if emoji_iter.peek().is_some() {
        for emoji in emoji_iter {
            msg.react(&ctx, emoji).await?;
        }
    } else if let Ok(num_reactions) = args.single::<u8>() {
        for i in 0..num_reactions.min(26) {
            msg.react(&ctx, emoji::nth_letter(i)).await?;
        }
    } else {
        msg.react(&ctx, '👍').await?;
        msg.react(&ctx, '👎').await?;
    }
    Ok(())
}

#[command]
#[owners_only]
async fn quit(ctx: &Context, _: &Message, _: Args) -> CommandResult {
    serenity_utils::shut_down(&ctx).await;
    Ok(())
}

#[command]
#[owners_only] //TODO allow any admin to use this command
async fn update(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    if let Some((world_name, _)) = ctx.data.read().await.get::<Config>().expect("missing config").wurstminebot.world_channels.iter().find(|(_, &chan_id)| chan_id == msg.channel_id) {
        msg.reply(ctx, MessageBuilder::default().push("Updating ").push_safe(world_name).push(" world…")).await?;
        World::new(world_name).update(VersionSpec::LatestRelease).await?;
        msg.reply_ping(ctx, "Done!").await?;
    } else {
        msg.reply(ctx, "This channel has no associated Minecraft world.").await?;
    }
    Ok(())
}

#[command]
async fn veto(ctx: &Context, _: &Message, args: Args) -> CommandResult {
    let data = ctx.data.read().await;
    let pool = data.get::<Database>().expect("missing database connection");
    let mut cmd = args.message();
    let mut builder = MessageBuilder::default();
    builder.push("invite for ");
    match parse::eat_person(&mut cmd, &pool).await? {
        Some(person) => { builder.push(person.mention()); } //TODO make sure remaining command is empty (or only whitespace), validate veto period, kick person from guild and remove from whitelist
        None => { builder.push_mono_safe(cmd); }
    }
    builder.push(" has been vetoed");
    GENERAL.say(&ctx, builder).await?;
    Ok(())
}

#[group]
#[commands(
    iam,
    iamn,
    ping,
    poll,
    quit,
    update,
    veto,
)]
struct Main;
