use {
    std::collections::BTreeSet,
    chrono::{
        Duration,
        prelude::*,
    },
    itertools::Itertools as _,
    serenity::{
        prelude::*,
        utils::{
            Colour,
            MessageBuilder,
        },
    },
    serenity_utils::RwFuture,
    tokio::time::sleep,
    crate::{
        Error,
        GENERAL,
        people::PersonId,
        util::join,
    },
};

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct Event {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub kind: EventKind,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    Tour {
        area: Option<String>,
        guests: Vec<PersonId>,
    },
}

impl EventKind {
    fn discord_title(&self) -> String {
        match self {
            Self::Tour { guests, area: Some(area) } => MessageBuilder::default()
                .push("tour of ")
                .push_safe(area)
                .push(" for ")
                .push(join(guests.iter().map(|guest| guest.mention())).unwrap_or_else(|| format!("no one")))
                .build(),
            Self::Tour { guests, area: None } => MessageBuilder::default()
                .push("server tour for ")
                .push(join(guests.iter().map(|guest| guest.mention())).unwrap_or_else(|| format!("no one")))
                .build(),
        }
    }

    fn discord_location(&self) -> String {
        match self {
            Self::Tour { area: Some(area), .. } => format!("{}\nWurstmineberg", area),
            Self::Tour { area: None, .. } => format!("spawn platform\nZucchini\nWurstmineberg"),
        }
    }
}

impl TypeMapKey for Event {
    type Value = BTreeSet<Event>;
}

pub async fn notifications(ctx_fut: RwFuture<Context>) -> Result<(), Error> {
    let now = Utc::now();
    let ctx = ctx_fut.read().await;
    let data = (*ctx).data.read().await;
    let events = data.get::<Event>().expect("missing events");
    let mut unnotified = events.iter()
        .filter(|event| event.start - Duration::minutes(30) >= now)
        .collect_vec();
    while !unnotified.is_empty() {
        let event = unnotified.remove(0);
        if let Ok(duration) = (event.start - now).to_std() {
            sleep(duration).await;
        }
        GENERAL.send_message(&*ctx, |m| m
            .content(format!("Event starting <t:{}:R>", event.start.timestamp()))
            .add_embed(|e| e
                .colour(Colour(8794372))
                .title(event.kind.discord_title())
                .description(event.kind.discord_location())
                .field("starts", format!("<t:{}:F>", event.start.timestamp()), false)
                .field("ends", format!("<t:{}:F>", event.end.timestamp()), false)
            )
        ).await?;
    }
    Ok(())
}