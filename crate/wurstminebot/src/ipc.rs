use {
    serenity::prelude::*,
    crate::WURSTMINEBERG,
};

serenity_utils::ipc! {
    use serenity::model::prelude::*;

    const PORT: u16 = 18809;

    /// Sends the given message, unescaped, to the given channel.
    async fn channel_msg(ctx: &Context, channel: ChannelId, msg: String) -> Result<(), String> {
        channel.say(ctx, msg).await.map_err(|e| format!("failed to send channel message: {}", e))?;
        Ok(())
    }

    /// Shuts down the bot and cleanly exits the program.
    async fn quit(ctx: &Context) -> Result<(), String> {
        serenity_utils::shut_down(&ctx).await;
        Ok(())
    }

    /// Changes the display name for the given user in the Gefolge guild to the given string.
    ///
    /// If the given string is equal to the user's username, the display name will instead be removed.
    async fn set_display_name(ctx: &Context, user_id: UserId, new_display_name: String) -> Result<(), String> {
        let user = user_id.to_user(ctx).await.map_err(|e| format!("failed to get user for set-display-name: {}", e))?;
        WURSTMINEBERG.edit_member(ctx, &user, |e| e.nickname(if user.name == new_display_name { "" } else { &new_display_name })).await.map_err(|e| match e {
            serenity::Error::Http(e) => if let HttpError::UnsuccessfulRequest(response) = *e {
                format!("failed to set display name: {:?}", response)
            } else {
                e.to_string()
            },
            _ => e.to_string()
        })
    }
}
