use {
    std::{
        convert::Infallible as Never,
        fmt,
        io,
        str::FromStr,
        sync::Arc
    },
    chase::{
        ChaseError,
        Chaser
    },
    //chrono::prelude::*,
    derive_more::From,
    futures::{
        compat::Stream01CompatExt as _,
        future::try_join_all,
        prelude::*,
        stream::{
            self,
            Stream
        }
    },
    itertools::Itertools as _,
    lazy_static::lazy_static,
    pin_utils::pin_mut,
    regex::Regex,
    serenity::{
        prelude::*,
        utils::MessageBuilder
    },
    systemd_minecraft::World,
    tokio::{
        fs::File,
        io::BufReader,
        prelude::*,
        task::JoinError
    },
    crate::util::ResultNeverExt as _
};

lazy_static! {
    static ref CHAT_LINE: Regex = Regex::new("^<([A-Za-z0-9_]{3,16})> (.+)$").expect("failed to parse chat line regex");
    static ref CHAT_ACTION_LINE: Regex = Regex::new("^\\* ([A-Za-z0-9_]{3,16}) (.+)$").expect("failed to parse chat action line regex");
    static ref REGULAR_LINE: Regex = Regex::new("^([0-9]+-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}) \\[([^]]+)/(INFO|WARN|ERROR)\\]: (.+)$").expect("failed to parse regular line regex");
}

#[derive(Debug, From)]
pub enum Error {
    /// The `futures::sync::mpsc::Receiver` returned by the `chase` crate yielded an error.
    Channel,
    Chase(ChaseError),
    FollowEnded,
    Io(io::Error),
    NoWorlds, //TODO remove once `handle` automatically handles new worlds as they are created
    Serenity(serenity::Error),
    Task(JoinError)
}

impl From<Never> for Error {
    fn from(never: Never) -> Error {
        match never {}
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Channel => write!(f, "unknown error in log handler"),
            Error::Chase(e) => write!(f, "error in log handler: {}", e),
            Error::FollowEnded => write!(f, "log handler returned unexpectedly"),
            Error::Io(e) => write!(f, "I/O error in log handler: {}", e),
            Error::NoWorlds => write!(f, "failed to start log handler: no worlds configured"),
            Error::Serenity(e) => write!(f, "error in log handler: {}", e),
            Error::Task(e) => write!(f, "error in log handler: {}", e)
        }
    }
}

enum Thread {
    Server,
    Unknown(String)
}

impl FromStr for Thread {
    type Err = Never;

    fn from_str(s: &str) -> Result<Thread, Never> {
        Ok(match s {
            "Server thread" => Thread::Server,
            _ => Thread::Unknown(s.to_owned())
        })
    }
}

enum Level {
    Info,
    Warn,
    Error
}

impl FromStr for Level {
    type Err = ();

    fn from_str(s: &str) -> Result<Level, ()> {
        match s {
            "INFO" => Ok(Level::Info),
            "WARN" => Ok(Level::Warn),
            "ERROR" => Ok(Level::Error),
            _ => Err(())
        }
    }
}

enum RegularLine {
    Chat {
        sender: String,
        msg: String,
        is_action: bool
    },
    Unknown(String)
}

impl FromStr for RegularLine {
    type Err = Never;

    fn from_str(s: &str) -> Result<RegularLine, Never> {
        Ok(if let Some(captures) = CHAT_LINE.captures(s) {
            RegularLine::Chat {
                sender: captures[1].to_owned(),
                msg: captures[2].to_owned(),
                is_action: false
            }
        } else if let Some(captures) = CHAT_ACTION_LINE.captures(s) {
            RegularLine::Chat {
                sender: captures[1].to_owned(),
                msg: captures[2].to_owned(),
                is_action: true
            }
        } else {
            RegularLine::Unknown(s.to_owned())
        })
    }
}

enum Line {
    Regular {
        //timestamp: DateTime<Utc>,
        //thread: Thread,
        //level: Level,
        content: RegularLine
    },
    Unknown(String)
}

impl Line {
    fn parse_regular(s: &str) -> Option<Line> {
        let captures = REGULAR_LINE.captures(s)?;
        Some(Line::Regular {
            //timestamp: Utc.datetime_from_str(&captures[1], "%Y-%m-%d %H:%M:%S").ok()?,
            //thread: captures[2].parse().never_unwrap(),
            //level: captures[3].parse().expect("level that matches regex should parse"),
            content: captures[4].parse().never_unwrap()
        })
    }
}

impl FromStr for Line {
    type Err = Never;

    fn from_str(s: &str) -> Result<Line, Never> {
        Ok(Line::parse_regular(s).unwrap_or_else(|| Line::Unknown(s.to_owned())))
    }
}

/// Follows the log of the given world, starting after the last line break at the time the stream is started.
fn follow(world: &World) -> impl Stream<Item = Result<Line, Error>> {
    let log_path = world.dir().join("logs/latest.log");
    stream::once(async {
        let f: File = File::open(&log_path).await?; //DEBUG
        let init_lines = BufReader::new(f).lines().try_fold(0, |acc, _| async move { Ok(acc + 1) }).await?;
        let mut chaser = Chaser::new(log_path);
        chaser.line = chase::Line(init_lines);
        let (rx, _ /*handle*/) = chaser.run_stream()?; //TODO handle errors in the stream using `handle`
        Ok::<_, Error>(rx.compat().map_err(|()| Error::Channel).and_then(|(line, _, _)| async move { Ok(line.parse()?) }))
    }).try_flatten()
}

pub async fn handle(ctx_arc: Arc<Mutex<Option<Context>>>) -> Result<Never, Error> { //TODO dynamically update handled worlds as they are added/removed
    let mut handles = Vec::default();
    for world in World::all()? {
        handles.push(tokio::spawn(handle_world(ctx_arc.clone(), world)));
    }
    match try_join_all(handles).await?.pop() {
        Some(Ok(never)) => match never {},
        Some(Err(e)) => return Err(e),
        None => {}
    }
    Err(Error::NoWorlds)
}

async fn handle_world(ctx_arc: Arc<Mutex<Option<Context>>>, world: World) -> Result<Never, Error> {
    let follower = follow(&world);
    pin_mut!(follower);
    while let Some(line) = follower.try_next().await? {
        match line {
            Line::Regular { content, .. } => match content {
                RegularLine::Chat { sender, msg, is_action } => {
                    if let Some(ctx) = ctx_arc.lock().as_ref() {
                        if let Some(chan_id) = ctx.data.read().get::<crate::Config>().expect("missing config").wurstminebot.world_channels.get(&world.to_string()) {
                            if let Ok(webhook) = chan_id.webhooks(ctx)?.into_iter().exactly_one() {
                                webhook.execute(ctx, false, |w| w
                                    //TODO set avatar_url to player head
                                    .content(if is_action {
                                        let mut builder = MessageBuilder::default();
                                        builder.push_italic_safe(msg);
                                        builder
                                    } else {
                                        let mut builder = MessageBuilder::default();
                                        builder.push_safe(msg);
                                        builder
                                    })
                                    .username(sender) //TODO use Discord nickname instead of Minecraft nickname?
                                )?;
                            }
                        }
                    }
                }
                RegularLine::Unknown(_) => {} // ignore all other lines for now
            },
            Line::Unknown(_) => {} // ignore all other lines for now
        }
    }
    Err(Error::FollowEnded)
}
