use {
    std::io,
    chrono::prelude::*,
    derive_more::From,
    futures::stream::TryStreamExt as _,
    ics::{
        ICalendar,
        properties::{
            DtEnd,
            DtStart,
            Location,
            Summary,
        },
    },
    rocket::{
        Request,
        Rocket,
        State,
        http::ContentType,
        response::{
            Debug,
            Responder,
            content::Custom,
        },
    },
    serenity::prelude::*,
    serenity_utils::RwFuture,
    sqlx::types::Json,
    crate::{
        Database,
        cal::{
            Event,
            EventKind,
        },
    },
};

#[derive(Debug, From)]
enum Error {
    Io(io::Error),
    Sql(sqlx::Error),
}

impl<'r> Responder<'r, 'static> for Error {
    fn respond_to(self, request: &'r Request<'_>) -> rocket::response::Result<'static> {
        Debug(self).respond_to(request)
    }
}

fn ics_datetime<Tz: TimeZone>(datetime: DateTime<Tz>) -> String {
    format!("{}", datetime.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ"))
}

#[rocket::get("/api/v3/calendar.ics")]
async fn calendar(ctx_fut: &State<RwFuture<Context>>) -> Result<Custom<Vec<u8>>, Error> {
    let mut cal = ICalendar::new("2.0", concat!("wurstmineberg.de/", env!("CARGO_PKG_VERSION")));
    let ctx = ctx_fut.read().await;
    let data = (*ctx).data.read().await;
    let pool = data.get::<Database>().expect("missing database connection");
    let mut events = sqlx::query_as!(Event, r#"SELECT id, start_time, end_time, kind as "kind: Json<EventKind>" FROM calendar"#).fetch(pool);
    while let Some(event) = events.try_next().await? {
        let mut cal_event = ics::Event::new(format!("event{}@wurstmineberg.de", event.id), ics_datetime(Utc::now()));
        cal_event.push(Summary::new(event.title(pool).await));
        cal_event.push(Location::new(event.ics_location()));
        cal_event.push(DtStart::new(ics_datetime(event.start_time)));
        cal_event.push(DtEnd::new(ics_datetime(event.end_time)));
        cal.add_event(cal_event);
    }
    let mut buf = Vec::default();
    cal.write(&mut buf)?; //TODO async/spawn_blocking?
    Ok(Custom(ContentType::Calendar, buf))
}

pub fn rocket(ctx_fut: RwFuture<Context>) -> Rocket<rocket::Build> {
    rocket::custom(rocket::Config {
        port: 24810,
        ..rocket::Config::default()
    })
    .manage(ctx_fut)
    .mount("/", rocket::routes![calendar])
}
