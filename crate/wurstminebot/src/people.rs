//! Model types.

use {
    futures::stream::{
        StreamExt as _,
        TryStreamExt as _,
    },
    serde_json::json,
    serenity::model::prelude::*,
    sqlx::PgPool,
};

pub enum PersonId {
    Discord(UserId),
    LegacyWurstmineberg(String),
}

impl PersonId {
    /// Attempts to look up a Person from a name and optional discriminator.
    ///
    /// The name is checked against the Discord username of each member of the Wurstmineberg Discord guild. If no discriminator is given, nicknames are also checked.
    ///
    /// Returns `None` if:
    ///
    /// * No one who is currently in the Wurstmineberg Discord guild has the given name as their username or nickname,
    /// * There are multiple People with the given name in the Wurstmineberg Discord guild and no discriminator was specified, or
    /// * The given Discord user is not a Person (e.g. wurstminebot).
    ///
    /// # Panics
    ///
    /// Panics if the discriminator is given but not a valid discriminator (i.e. if it's equal to 0 or greater than 9999).
    pub async fn from_discord(pool: &PgPool, name: &str, discriminator: Option<u16>) -> sqlx::Result<Option<UserId>> {
        let mut matching_ids = if let Some(discrim) = discriminator {
            if discrim == 0 || discrim > 9999 { panic!("invalid discriminator: {}", discriminator.unwrap()) }
            sqlx::query!("SELECT snowflake FROM people WHERE discorddata->'username' = $1 AND discorddata->'discriminator' = $2", json!(name), json!(i16::try_from(discrim).expect("just checked")))
                .fetch(pool)
                .map_ok(|row| UserId(row.snowflake.expect("found Person with discorddata but no snowflake") as u64))
                .boxed()
        } else {
            sqlx::query!("SELECT snowflake FROM people WHERE discorddata->'username' = $1 OR discorddata->'nick' = $1", json!(name))
                .fetch(pool)
                .map_ok(|row| UserId(row.snowflake.expect("found Person with discorddata but no snowflake") as u64))
                .boxed()
        };
        Ok(if let Some(first) = matching_ids.try_next().await? {
            if matching_ids.try_next().await?.is_some() {
                None
            } else {
                Some(first)
            }
        } else {
            None
        })
    }

    pub(crate) fn mention(&self) -> String {
        match self {
            Self::Discord(user_id) => user_id.mention().to_string(),
            Self::LegacyWurstmineberg(wmbid) => wmbid.clone(),
        }
    }
}
