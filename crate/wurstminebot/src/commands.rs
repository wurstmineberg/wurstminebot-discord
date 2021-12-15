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
    serenity_utils::{
        shut_down,
        slash::*,
    },
    systemd_minecraft::{
        VersionSpec,
        World,
    },
    crate::{
        ADMIN,
        Database,
        GENERAL,
        WURSTMINEBERG,
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

#[serenity_utils::slash_command(WURSTMINEBERG, allow_all)]
/// Give yourself a self-assignable role
async fn iam(ctx: &Context, member: &mut Member, #[serenity_utils(description = "the role to add")] role: Role) -> serenity::Result<&'static str> {
    if !ctx.data.read().await.get::<Config>().expect("missing self-assignable roles list").wurstminebot.self_assignable_roles.contains(&role.id) {
        return Ok("this role is not self-assignable") //TODO submit role list on command creation
    }
    if member.roles.contains(&role.id) {
        return Ok("you already have this role")
    }
    member.add_role(&ctx, role).await?;
    Ok("role added")
}

#[serenity_utils::slash_command(WURSTMINEBERG, allow_all)]
/// Remove a self-assignable role from yourself
async fn iamn(ctx: &Context, member: &mut Member, #[serenity_utils(description = "the role to remove")] role: Role) -> serenity::Result<&'static str> {
    if !ctx.data.read().await.get::<Config>().expect("missing self-assignable roles list").wurstminebot.self_assignable_roles.contains(&role.id) {
        return Ok("this role is not self-assignable") //TODO submit role list on command creation
    }
    if !member.roles.contains(&role.id) {
        return Ok("you already don't have this role")
    }
    member.remove_role(&ctx, role).await?;
    Ok("role removed")
}

#[serenity_utils::slash_command(WURSTMINEBERG, allow_all)]
/// Test if wurstminebot is online
fn ping() -> String {
    let mut rng = thread_rng();
    if rng.gen_bool(0.01) {
        format!("BWO{}{}G", "R".repeat(rng.gen_range(3..20)), "N".repeat(rng.gen_range(1..5)))
    } else {
        format!("pong")
    }
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

#[serenity_utils::slash_command(WURSTMINEBERG, allow(ADMIN))]
/// Shut down wurstminebot
async fn quit(ctx: &Context, interaction: &ApplicationCommandInteraction) -> serenity::Result<NoResponse> {
    interaction.create_interaction_response(ctx, |builder| builder.interaction_response_data(|data| data.content("shutting down…"))).await?;
    shut_down(&ctx).await;
    Ok(NoResponse)
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
    //TODO only allow current members to use this command
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
    poll,
    update,
    veto,
)] //TODO any-admin command to add a calendar event
struct Main;
