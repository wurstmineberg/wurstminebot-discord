use {
    std::collections::BTreeSet,
    chrono::{
        Duration,
        prelude::*,
    },
    itertools::Itertools as _,
    serenity::{
        prelude::*,
        utils::Colour,
    },
    serenity_utils::RwFuture,
    sqlx::PgPool,
    tokio::time::sleep,
    crate::{
        Database,
        Error,
        GENERAL,
        people::PersonId,
        util::join,
    },
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Event {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub kind: EventKind,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
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

impl TypeMapKey for Event {
    type Value = BTreeSet<Event>;
}

pub async fn notifications(ctx_fut: RwFuture<Context>) -> Result<(), Error> {
    let now = Utc::now();
    let ctx = ctx_fut.read().await;
    let mut unnotified = {
        let data = (*ctx).data.read().await;
        let events = data.get::<Event>().expect("missing events");
        events.iter()
            .filter(|event| event.start - Duration::minutes(30) >= now)
            .cloned()
            .collect_vec()
    };
    while !unnotified.is_empty() {
        let event = unnotified.remove(0);
        if let Ok(duration) = (event.start - Duration::minutes(30) - Utc::now()).to_std() {
            sleep(duration).await;
        }
        let title = {
            let data = (*ctx).data.read().await;
            let pool = data.get::<Database>().expect("missing database connection");
            event.kind.title(pool).await
        };
        GENERAL.send_message(&*ctx, |m| m
            .content(format!("event starting <t:{}:R>", event.start.timestamp()))
            .add_embed(|e| e
                .colour(Colour(8794372))
                .title(title)
                .description(event.kind.discord_location())
                .field("starts", format!("<t:{}:F>", event.start.timestamp()), false)
                .field("ends", format!("<t:{}:F>", event.end.timestamp()), false)
            )
        ).await?;
    }
    Ok(())
}
