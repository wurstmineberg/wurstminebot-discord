//! Model types.

use {
    diesel::prelude::*,
    serde_json::{
        Value as Json,
        json
    },
    serenity::model::prelude::*,
    crate::{
        Error,
        schema::people::dsl::*
    }
};

/// A Person is a member (or former member or invitee) of Wurstmineberg.
///
/// Information about People is stored in the `people` table in the `wurstmineberg` PostgreSQL database.
#[allow(unused)]
#[derive(Debug, Queryable)]
pub struct Person {
    id: i32,
    wmbid: Option<String>,
    snowflake: Option<i64>,
    active: bool,
    data: Option<Json>,
    version: i32,
    apikey: Option<String>,
    discorddata: Option<Json>
}

impl Person {
    /// Returns an iterator over all the People of Wurstmineberg.
    pub fn all(conn: &PgConnection) -> QueryResult<Vec<Person>> {
        people.load(conn)
    }

    /// Constructs a Person from a Discord user ID (“snowflake”). Returns `Ok<None>` if the given Discord user is not a Person or if the given snowflake does not correspond to a Discord user.
    pub fn from_snowflake(conn: &PgConnection, user_id: UserId) -> QueryResult<Option<Person>> {
        people.filter(snowflake.eq(Some(user_id.0 as i64))).first(conn).optional() // PostgreSQL doesn't have unsigned integer types
    }

    /// Attempts to construct a Person from a name and optional discriminator.
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
    pub fn from_discord(conn: &PgConnection, name: &str, discriminator: Option<u16>) -> QueryResult<Option<Person>> {
        let mut matching_people = people.filter(discorddata.is_not_null()).load::<Person>(conn)?;
        if let Some(discrim) = discriminator {
            if discrim == 0 || discrim > 9999 { panic!("invalid discriminator: {}", discriminator.unwrap()); }
            matching_people.retain(|person| {
                let discord_data = person.discorddata.as_ref().expect("discorddata was NULL");
                discord_data["username"].as_str().expect("discorddata.username wasn't a str") == name
                && discord_data["discriminator"].as_u64().expect("discorddata.discriminator wasn't a u64") == u64::from(discrim)
            });
        } else {
            matching_people.retain(|person| {
                let discord_data = person.discorddata.as_ref().expect("discorddata was NULL");
                discord_data["username"].as_str().expect("discorddata.username wasn't a str") == name
                || discord_data["nick"].as_str().map_or(false, |nick| nick == name)
            })
        }
        Ok(if matching_people.len() == 1 {
            matching_people.pop()
        } else {
            None
        })
    }

    pub fn minecraft_nick(&self) -> Option<&str> {
        self.data.as_ref()?.pointer("/minecraft/nicks/0")?.as_str()
    }

    /// Deletes the Discord metadata (username, discriminator, nickname, guild join date, and roles) for the Person with the given Discord snowflake, if any.
    ///
    /// This should be called when the Person leaves the guild.
    ///
    /// If successful, the updated Person is returned.
    pub fn remove_discord_data(conn: &PgConnection, user: impl Into<UserId>) -> QueryResult<Option<Person>> {
        diesel::update(people.filter(snowflake.eq(Some(user.into().0 as i64))))
            .set(discorddata.eq(None::<Json>))
            .get_result(conn)
            .optional()
    }

    pub fn twitch_nick(&self) -> Option<&str> {
        self.data.as_ref()?.pointer("/twitch/login")?.as_str()
    }

    /// Updates the Discord metadata (username, discriminator, nickname, guild join date, and roles) for the Person with the given Discord snowflake, if any.
    ///
    /// If successful, the updated Person is returned.
    pub fn update_discord_data(conn: &PgConnection, member: &Member) -> Result<Option<Person>, Error> {
        //TODO update display name in data column
        let user = member.user.read().clone();
        diesel::update(people.filter(snowflake.eq(Some(user.id.0 as i64))))
            .set(discorddata.eq(Some(json!({
                "avatar": user.avatar_url(),
                "discriminator": user.discriminator,
                "joined": if let Some(ref join_date) = member.joined_at { join_date } else { return Err(Error::MissingJoinDate) },
                "nick": &member.nick,
                "roles": &member.roles,
                "username": user.name
            }))))
            .get_result(conn)
            .optional()
            .map_err(Error::from)
    }
}

impl Mentionable for Person {
    fn mention(&self) -> String {
        match (self.snowflake, &self.wmbid) {
            (Some(flake), _) => UserId(flake as u64).mention(),
            (None, &Some(ref wmb_id)) => {
                if let Some(Json::String(name)) = self.data.clone().and_then(|mut person_data| person_data.get_mut("name").map(Json::take)) {
                    name
                } else {
                    wmb_id.clone()
                }
            }
            (None, &None) => { panic!("tried to mention user with no IDs"); }
        }
    }
}
