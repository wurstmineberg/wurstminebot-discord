use {
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

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) id: i32,
    pub(crate) start_time: DateTime<Utc>,
    pub(crate) end_time: DateTime<Utc>,
    pub(crate) kind: Json<EventKind>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventKind {
    Tour {
        area: Option<String>,
        guests: Vec<PersonId>,
    },
}

impl EventKind {
    pub(crate) async fn title(&self, pool: &PgPool) -> String {
        match self {
            Self::Tour { guests, area } => {
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
        }
    }

    pub(crate) fn ics_location(&self) -> String {
        match self {
            Self::Tour { area: Some(area), .. } => format!("{}\nWurstmineberg", area),
            Self::Tour { area: None, .. } => format!("spawn platform\nZucchini\nWurstmineberg"),
        }
    }

    fn discord_location(&self) -> String {
        match self {
            Self::Tour { area: Some(area), .. } => format!("{}\nWurstmineberg", area),
            Self::Tour { area: None, .. } => format!("spawn platform\n[Zucchini](https://wurstmineberg.de/wiki/renascence#zucchini)\nWurstmineberg"),
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
            event.kind.0.title(pool).await
        };
        GENERAL.send_message(&*ctx, |m| m
            .content(format!("event starting <t:{}:R>", event.start_time.timestamp()))
            .add_embed(|e| e
                .colour(Colour(8794372))
                .title(title)
                .description(event.kind.0.discord_location())
                .field("starts", format!("<t:{}:F>", event.start_time.timestamp()), false)
                .field("ends", format!("<t:{}:F>", event.end_time.timestamp()), false)
            )
        ).await?;
    }
    Ok(())
}
