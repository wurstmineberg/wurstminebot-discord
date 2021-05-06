use {
    minecraft::chat::Chat,
    systemd_minecraft::World,
    crate::Error,
};

pub async fn tellraw(world: &World, rcpt: &str, msg: &Chat) -> Result<String, Error> { //TODO move to systemd-minecraft
    Ok(world.command(&format!("tellraw {} {}", rcpt, msg)).await?)
}
