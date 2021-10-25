use {
    std::str::FromStr as _,
    itertools::Itertools as _,
    lazy_regex::regex_captures,
    serenity::model::prelude::*,
    sqlx::PgPool,
    crate::people::PersonId,
};

pub fn eat_optional_prefix(cmd: &mut &str, prefix: char) -> bool {
    if cmd.starts_with(prefix) {
        *cmd = &cmd[1..];
        true
    } else {
        false
    }
}

pub async fn eat_person(cmd: &mut &str, pool: &PgPool) -> sqlx::Result<Option<PersonId>> {
    if let Some(user_id) = eat_user_mention(cmd) {
        return Ok(Some(PersonId::Discord(user_id)))
    }
    if cmd.starts_with('@') && cmd.contains('#') {
        if let Some((full_match, username, discrim)) = regex_captures!("^@([^@#:]{2,32})#([0-9]{4})?", cmd) { //TODO better compliance with https://discordapp.com/developers/docs/resources/user
            let discrim = if discrim.is_empty() {
                None
            } else {
                Some(discrim.parse().expect("failed to convert Discord discriminator to integer"))
            };
            if let Some(user_id) = PersonId::from_discord(pool, username, discrim).await? {
                *cmd = &cmd[full_match.len()..];
                return Ok(Some(PersonId::Discord(user_id)))
            }
        }
    }
    if let Some(word) = next_word(&cmd) {
        let mut word = &word[..];
        eat_optional_prefix(&mut word, '@');
        if let Some(user_id) = PersonId::from_discord(pool, &word, None).await? {
            eat_word(cmd);
            return Ok(Some(PersonId::Discord(user_id)))
        }
    }
    //TODO parse legacy Wurstmineberg IDs
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
