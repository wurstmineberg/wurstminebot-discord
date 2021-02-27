use {
    std::fmt,
    systemd_minecraft::World,
    serde::Serialize,
    crate::Error
};

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White
}

#[derive(Default, Serialize)]
pub struct Chat {
    text: String,
    color: Option<Color>
}

impl Chat {
    pub fn color(&mut self, color: Color) -> &mut Chat {
        self.color = Some(color);
        self
    }
}

impl From<String> for Chat {
    fn from(text: String) -> Chat {
        Chat { text, ..Chat::default() }
    }
}

impl fmt::Display for Chat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(self).map_err(|_| fmt::Error)?)
    }
}

pub async fn tellraw(world: &World, rcpt: &str, msg: &Chat) -> Result<String, Error> {
    Ok(world.command(&format!("tellraw {} {}", rcpt, msg)).await?)
}
