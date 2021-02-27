#![allow(missing_docs)] //TODO

use {
    std::str::FromStr as _,
    diesel::prelude::*,
    itertools::Itertools as _,
    regex::Regex,
    serenity::model::prelude::*,
    crate::people::Person,
};

pub fn eat_optional_prefix(cmd: &mut &str, prefix: char) -> bool {
    if cmd.starts_with(prefix) {
        *cmd = &cmd[1..];
        true
    } else {
        false
    }
}

pub fn eat_person(cmd: &mut &str, conn: &PgConnection) -> QueryResult<Option<Person>> {
    let original_cmd = *cmd;
    if let Some(user_id) = eat_user_mention(cmd) {
        return match Person::from_snowflake(conn, user_id) {
            Ok(opt_person) => Ok(opt_person),
            Err(e) => {
                *cmd = original_cmd;
                Err(e)
            }
        }
    }
    if cmd.starts_with('@') && cmd.contains('#') {
        let username_regex = Regex::new("^@([^@#:]{2,32})#([0-9]{4})?").expect("failed to compile username regex"); //TODO better compliance with https://discordapp.com/developers/docs/resources/user
        if let Some(captures) = username_regex.captures(cmd) {
            if let Some(person) = Person::from_discord(conn, &captures[1], captures.get(2).map(|discr| discr.as_str().parse().expect("failed to convert Discord discriminator to integer")))? {
                *cmd = &cmd[captures[0].len()..];
                return Ok(Some(person))
            }
        }
    }
    if let Some(word) = next_word(&cmd) {
        let mut word = &word[..];
        eat_optional_prefix(&mut word, '@');
        if let Some(person) = Person::from_discord(conn, &word, None)? {
            eat_word(cmd);
            return Ok(Some(person))
        }
    }
    Ok(None)
}

/// Returns a role given its mention or name, but only if it's the entire command.
pub async fn eat_role_full(cmd: &mut &str, guild: Option<Guild>) -> Option<RoleId> {
    let original_cmd = *cmd;
    if let Some(role_id) = eat_role_mention(cmd) {
        if cmd.is_empty() {
            Some(role_id)
        } else {
            *cmd = original_cmd;
            None
        }
    } else if let Some(guild) = guild {
        guild.roles
            .iter()
            .filter_map(|(&role_id, role)| if role.name == *cmd { Some(role_id) } else { None })
            .exactly_one()
            .ok()
    } else {
        None
    }
}

pub fn eat_role_mention(cmd: &mut &str) -> Option<RoleId> {
    if !cmd.starts_with('<') || !cmd.contains('>') {
        return None
    }
    let mut maybe_mention = String::default();
    let mut chars = cmd.chars();
    while let Some(c) = chars.next() {
        maybe_mention.push(c);
        if c == '>' {
            if let Ok(id) = RoleId::from_str(&maybe_mention) {
                eat_word(cmd);
                return Some(id)
            }
            return None
        }
    }
    None
}

pub fn eat_user_mention(cmd: &mut &str) -> Option<UserId> {
    if !cmd.starts_with('<') || !cmd.contains('>') {
        return None
    }
    let mut maybe_mention = String::default();
    let mut chars = cmd.chars();
    while let Some(c) = chars.next() {
        maybe_mention.push(c);
        if c == '>' {
            if let Ok(id) = UserId::from_str(&maybe_mention) {
                eat_word(cmd);
                return Some(id)
            }
            return None
        }
    }
    None
}

pub fn eat_whitespace(cmd: &mut &str) {
    while eat_optional_prefix(cmd, ' ') {}
}

fn eat_word(cmd: &mut &str) -> Option<String> {
    if let Some(word) = next_word(&cmd) {
        *cmd = &cmd[word.len()..];
        eat_whitespace(cmd);
        Some(word)
    } else {
        None
    }
}

pub fn next_word(cmd: &str) -> Option<String> {
    let mut word = String::default();
    for c in cmd.chars() {
        if c == ' ' { break }
        word.push(c);
    }
    if word.is_empty() { None } else { Some(word) }
}
