use {
    std::io,
    chrono::prelude::*,
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
        Rocket,
        State,
        http::ContentType,
        response::{
            Debug,
            content::Custom,
        },
    },
    serenity::prelude::*,
    serenity_utils::RwFuture,
    crate::{
        Database,
        cal::Event,
    },
};

fn ics_datetime<Tz: TimeZone>(datetime: DateTime<Tz>) -> String {
    format!("{}", datetime.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ"))
}

#[rocket::get("/api/v3/calendar.ics")]
async fn calendar(ctx_fut: &State<RwFuture<Context>>) -> Result<Custom<Vec<u8>>, Debug<io::Error>> {
    let mut cal = ICalendar::new("2.0", concat!("wurstmineberg.de/", env!("CARGO_PKG_VERSION")));
    let ctx = ctx_fut.read().await;
    let data = (*ctx).data.read().await;
    let pool = data.get::<Database>().expect("missing database connection");
    let events = data.get::<Event>().expect("missing events");
    for (i, event) in events.iter().enumerate() {
        let mut cal_event = ics::Event::new(format!("event{}@wurstmineberg.de", i), ics_datetime(Utc::now()));
        cal_event.push(Summary::new(event.kind.title(pool).await));
        cal_event.push(Location::new(event.kind.ics_location()));
        cal_event.push(DtStart::new(ics_datetime(event.start)));
        cal_event.push(DtEnd::new(ics_datetime(event.end)));
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
