use {
    std::borrow::Cow,
    chrono::{
        Duration,
        prelude::*,
    },
    serde::Deserialize,
    serenity::{
        prelude::*,
        utils::Colour,
    },
    serenity_utils::RwFuture,
    sqlx::{
        PgPool,
        types::Json,
    },
    tokio::time::sleep,
    crate::{
        Database,
        Error,
        GENERAL,
        people::PersonId,
        util::join,
    },
};

#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventKind {
    Minigame {
        minigame: String,
    },
    #[serde(rename_all = "camelCase")]
    Renascence {
        settlement: String,
        hub_coords: [i16; 2],
    },
    RenascenceDragonFight {
        settlement: String,
    },
    Tour {
        area: Option<String>,
        guests: Vec<PersonId>,
    },
    Usc {
        season: usize,
    },
    Other {
        title: String,
        location: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) id: i32,
    pub(crate) start_time: DateTime<Utc>,
    pub(crate) end_time: DateTime<Utc>,
    pub(crate) kind: Json<EventKind>,
}

impl Event {
    pub(crate) async fn title(&self, pool: &PgPool) -> String {
        match self.kind.0 {
            EventKind::Minigame { ref minigame } => format!("Minigame Night: {}", minigame),
            EventKind::Renascence { ref settlement, .. } => format!("Renascence: {}", settlement),
            EventKind::RenascenceDragonFight { ref settlement } => format!("{} dragon fight", settlement),
            EventKind::Tour { ref guests, ref area } => {
                let mut guest_names = Vec::default();
                for guest in guests {
                    guest_names.push(guest.display(pool).await);
                }
                if let Some(area) = area {
                    format!("tour of {} for {}", area, join(guest_names).unwrap_or_else(|| format!("no one")))
                } else {
                    format!("server tour for {}", join(guest_names).unwrap_or_else(|| format!("no one")))
                }
            }
            EventKind::Usc { season } => format!("Ultra Softcore season {}", season),
            EventKind::Other { ref title, .. } => title.to_owned(),
        }
    }

    pub(crate) fn ics_location(&self) -> Option<Cow<'static, str>> {
        match self.kind.0 {
            EventKind::Minigame { .. } => Some(Cow::Borrowed("minigame.wurstmineberg.de")),
            EventKind::Renascence { hub_coords: [x, z], .. } => Some(Cow::Owned(format!("Hub {}, {}\nThe Nether\nWurstmineberg", x, z))),
            EventKind::RenascenceDragonFight { ref settlement } => Some(Cow::Owned(format!("{}\nWurstmineberg", settlement))),
            EventKind::Tour { area: Some(ref area), .. } => Some(Cow::Owned(format!("{}\nWurstmineberg", area))),
            EventKind::Tour { area: None, .. } => Some(Cow::Borrowed(if self.start_time >= Utc.ymd(2019, 4, 7).and_hms(0, 0, 0) {
                "spawn platform\nZucchini\nWurstmineberg"
            } else {
                "Platz des Ursprungs\nWurstmineberg"
            })),
            EventKind::Usc { .. } => Some(Cow::Borrowed("usc.wurstmineberg.de")),
            EventKind::Other { location: Some(ref loc), .. } => Some(Cow::Owned(loc.to_owned())),
            EventKind::Other { location: None, .. } => None,
        }
    }

    fn discord_location(&self) -> Option<Cow<'static, str>> {
        match self.kind.0 {
            EventKind::Minigame { .. } => Some(Cow::Borrowed("minigame.wurstmineberg.de")),
            EventKind::Renascence { hub_coords: [x, z], .. } => Some(Cow::Owned(format!("[Hub](https://wurstmineberg.de/wiki/nether-hub-system) {}, {}\nThe Nether\nWurstmineberg", x, z))),
            EventKind::RenascenceDragonFight { ref settlement } => Some(Cow::Owned(format!("[{}](https://wurstmineberg.de/renascence#{})\nWurstmineberg", settlement, settlement.to_lowercase()))),
            EventKind::Tour { area: Some(ref area), .. } => Some(Cow::Owned(format!("{}\nWurstmineberg", area))),
            EventKind::Tour { area: None, .. } => Some(Cow::Borrowed(if self.start_time >= Utc.ymd(2019, 4, 7).and_hms(0, 0, 0) {
                "spawn platform\n[Zucchini](https://wurstmineberg.de/wiki/renascence#zucchini)\nWurstmineberg"
            } else {
                "[Platz des Ursprungs](https://wurstmineberg.de/wiki/old-spawn#platz-des-ursprungs)\nWurstmineberg"
            })),
            EventKind::Usc { .. } => Some(Cow::Borrowed("usc.wurstmineberg.de")), //TODO linkify via menu bar/systray app?
            EventKind::Other { location: Some(ref loc), .. } => Some(Cow::Owned(loc.to_owned())),
            EventKind::Other { location: None, .. } => None,
        }
    }
}

pub async fn notifications(ctx_fut: RwFuture<Context>) -> Result<(), Error> {
    let ctx = ctx_fut.read().await;
    let mut unnotified = {
        let data = (*ctx).data.read().await;
        let pool = data.get::<Database>().expect("missing database connection");
        let now = Utc::now();
        sqlx::query_as!(Event, r#"SELECT id, start_time, end_time, kind as "kind: Json<EventKind>" FROM calendar WHERE start_time > $1 ORDER BY start_time"#, now + Duration::minutes(30)).fetch_all(pool).await?
    };
    while !unnotified.is_empty() {
        let event = unnotified.remove(0);
        if let Ok(duration) = (event.start_time - Duration::minutes(30) - Utc::now()).to_std() {
            sleep(duration).await;
        }
        let title = {
            let data = (*ctx).data.read().await;
            let pool = data.get::<Database>().expect("missing database connection");
            event.title(pool).await
        };
        GENERAL.send_message(&*ctx, |m| m
            .content(format!("event starting <t:{}:R>", event.start_time.timestamp()))
            .add_embed(|e| {
                e.colour(Colour(8794372));
                e.title(title);
                if let Some(loc) = event.discord_location() {
                    e.description(loc);
                }
                e.field("starts", format!("<t:{}:F>", event.start_time.timestamp()), false);
                e.field("ends", format!("<t:{}:F>", event.end_time.timestamp()), false);
                e
            })
        ).await?;
    }
    Ok(())
}
