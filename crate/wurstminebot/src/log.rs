use {
    std::{
        collections::HashMap,
        convert::Infallible as Never,
        path::PathBuf,
        str::FromStr,
        sync::Arc,
        time::Duration,
    },
    chase::Chaser,
    futures::{
        future::try_join_all,
        pin_mut,
        prelude::*,
        stream::{
            self,
            Stream,
        },
    },
    itertools::Itertools as _,
    lazy_regex::{
        regex_captures,
        regex_replace_all,
    },
    regex::Regex,
    serde::Deserialize,
    serenity::{
        all::ExecuteWebhook,
        prelude::*,
        utils::MessageBuilder,
    },
    serenity_utils::RwFuture,
    systemd_minecraft::World,
    tokio::{
        io::{
            self,
            AsyncBufReadExt as _,
            BufReader,
        },
        sync::RwLock,
    },
    tokio_stream::wrappers::{
        LinesStream,
        ReceiverStream,
    },
    tokio_util::io::StreamReader,
    url::Url,
    wheel::{
        fs::{
            self,
            File,
        },
        io_error_from_reqwest,
        traits::ReqwestResponseExt as _,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)] Chase(#[from] chase::Error),
    #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] Minecraft(#[from] systemd_minecraft::Error),
    #[error(transparent)] Regex(#[from] regex::Error),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Task(#[from] tokio::task::JoinError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error(transparent)] Zip(#[from] async_zip::error::ZipError),
    /// The `futures::sync::mpsc::Receiver` returned by the `chase` crate yielded an error.
    #[error("unknown error in log handler")]
    Channel,
    #[error("log handler returned unexpectedly")]
    FollowEnded,
    #[error("no en_us language file in Minecraft client jar")]
    MissingLangFile,
    #[error("Minecraft version not found in launcher manifest")]
    MissingVersion,
    #[error("failed to start log handler: no worlds configured")]
    NoWorlds, //TODO remove once `handle` automatically handles new worlds as they are created
}

impl From<Never> for Error {
    fn from(never: Never) -> Self {
        match never {}
    }
}

enum Thread {
    Server,
    Unknown(String),
}

impl FromStr for Thread {
    type Err = Never;

    fn from_str(s: &str) -> Result<Thread, Never> {
        Ok(match s {
            "Server thread" => Thread::Server,
            _ => Thread::Unknown(s.to_owned()),
        })
    }
}

enum Level {
    Info,
    Warn,
    Error,
}

impl FromStr for Level {
    type Err = ();

    fn from_str(s: &str) -> Result<Level, ()> {
        match s {
            "INFO" => Ok(Level::Info),
            "WARN" => Ok(Level::Warn),
            "ERROR" => Ok(Level::Error),
            _ => Err(()),
        }
    }
}

enum AdvancementKind {
    Challenge,
    Goal,
    Task,
}

enum RegularLine {
    ServerStart,
    Chat {
        sender: String,
        msg: String,
        is_action: bool,
    },
    Advancement {
        kind: AdvancementKind,
        player: String,
        advancement: String,
    },
    Death {
        msg: String,
    },
    Unknown(String),
}

struct FollowerState {
    http_client: reqwest::Client,
    minecraft_version: Option<String>,
    death_messages: HashMap<String, Regex>,
}

fn format_to_regex(format: &str) -> Result<Regex, regex::Error> {
    Regex::new(&format!("^{}$", regex_replace_all!("%[0-9]+\\$s", format, ".*")))
}

impl RegularLine {
    async fn parse(state: Arc<RwLock<FollowerState>>, s: &str) -> Result<Self, Error> {
        Ok(if let Some((_, version)) = regex_captures!("^Starting minecraft server version (.+)%", s) {
            let mut state = state.write().await;
            if state.minecraft_version.as_ref().map_or(true, |prev_version| prev_version != version) {
                state.minecraft_version = Some(version.to_owned());
                let client_jar_dir = PathBuf::from(format!("/opt/wurstmineberg/home/.minecraft-wurstmineberg/versions/{version}"));
                let client_jar_path = client_jar_dir.join(format!("{version}.jar"));
                if !fs::exists(&client_jar_path).await? {
                    #[derive(Deserialize)]
                    struct VersionManifestInfo {
                        id: String,
                        url: Url,
                    }

                    #[derive(Deserialize)]
                    struct VersionManifest {
                        versions: Vec<VersionManifestInfo>,
                    }

                    #[derive(Deserialize)]
                    struct VersionInfo {
                        downloads: VersionInfoDownloads,
                    }

                    #[derive(Deserialize)]
                    struct VersionInfoDownloads {
                        client: VersionInfoDownload,
                    }

                    #[derive(Deserialize)]
                    struct VersionInfoDownload {
                        url: Url,
                    }

                    fs::create_dir_all(&client_jar_dir).await?;
                    let version_manifest = state.http_client.get("https://launchermeta.mojang.com/mc/game/version_manifest.json")
                        .send().await?
                        .detailed_error_for_status().await?
                        .json_with_text_in_error::<VersionManifest>().await?;
                    let version_info = state.http_client.get(version_manifest.versions.into_iter().find(|iter_version| iter_version.id == version).ok_or(Error::MissingVersion)?.url)
                        .send().await?
                        .detailed_error_for_status().await?
                        .json_with_text_in_error::<VersionInfo>().await?;
                    io::copy_buf(&mut StreamReader::new(state.http_client.get(version_info.downloads.client.url).send().await?.detailed_error_for_status().await?.bytes_stream().map_err(io_error_from_reqwest)), &mut File::create(&client_jar_path).await?).await?;
                }
                let zip_file = async_zip::tokio::read::fs::ZipFileReader::new(client_jar_path).await?;
                let index = zip_file.file().entries().iter().position(|entry| entry.filename().as_str().map_or(false, |filename| filename == "assets/minecraft/lang/en_us.json")).ok_or(Error::MissingLangFile)?;
                let mut english = String::default();
                zip_file.reader_with_entry(index).await?.read_to_string_checked(&mut english).await?;
                state.death_messages = serde_json::from_str::<HashMap<&str, &str>>(&english)?
                    .into_iter()
                    .filter(|(key, _)| key.starts_with("death."))
                    .map(|(key, format)| Ok::<_, Error>((key.to_owned(), format_to_regex(format)?)))
                    .try_collect()?;
            }
            Self::ServerStart
        } else if let Some((_, sender, msg)) = regex_captures!("^(?:\\[Not Secure\\] )?<([A-Za-z0-9_]{3,16})> (.+)$", s) {
            Self::Chat {
                sender: sender.to_owned(),
                msg: msg.to_owned(),
                is_action: false,
            }
        } else if let Some((_, sender, msg)) = regex_captures!("^(?:\\[Not Secure\\] )?\\* ([A-Za-z0-9_]{3,16}) (.+)$", s) {
            Self::Chat {
                sender: sender.to_owned(),
                msg: msg.to_owned(),
                is_action: true,
            }
        } else if let Some((_, player, advancement)) = regex_captures!(r"^([A-Za-z0-9_]{3,16}) has completed the challenge \[(.+)\]$", s) {
            Self::Advancement {
                kind: AdvancementKind::Challenge,
                player: player.to_owned(),
                advancement: advancement.to_owned(),
            }
        } else if let Some((_, player, advancement)) = regex_captures!(r"^([A-Za-z0-9_]{3,16}) has reached the goal \[(.+)\]$", s) {
            Self::Advancement {
                kind: AdvancementKind::Goal,
                player: player.to_owned(),
                advancement: advancement.to_owned(),
            }
        } else if let Some((_, player, advancement)) = regex_captures!(r"^([A-Za-z0-9_]{3,16}) has made the advancement \[(.+)\]$", s) {
            Self::Advancement {
                kind: AdvancementKind::Task,
                player: player.to_owned(),
                advancement: advancement.to_owned(),
            }
        } else if state.read().await.death_messages.iter().any(|(_, regex)| regex.is_match(s)) {
            Self::Death {
                msg: s.to_owned(),
            }
        } else {
            Self::Unknown(s.to_owned())
        })
    }
}

enum Line {
    Regular {
        //timestamp: DateTime<Utc>,
        //thread: Thread,
        //level: Level,
        content: RegularLine,
    },
    Unknown(String),
}

impl Line {
    async fn parse(state: Arc<RwLock<FollowerState>>, s: &str) -> Result<Self, Error> {
        Ok(if let Some((_, _ /*timestamp*/, _ /*thread*/, _ /*level*/, content)) = regex_captures!("^([0-9]+-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}) \\[([^]]+)/(INFO|WARN|ERROR)\\]: (.+)$", s) {
            Self::Regular {
                //timestamp: Utc.datetime_from_str(timestamp, "%Y-%m-%d %H:%M:%S").ok()?,
                //thread: thread.parse().never_unwrap(),
                //level: level.parse().expect("level that matches regex should parse"),
                content: RegularLine::parse(state, content).await?,
            }
        } else {
            Self::Unknown(s.to_owned())
        })
    }
}

/// Follows the log of the given world, starting after the last line break at the time the stream is started.
fn follow(http_client: reqwest::Client, world: &World) -> impl Stream<Item = Result<Line, Error>> {
    let log_path = world.dir().join("logs/latest.log");
    stream::once(async {
        let init_lines = LinesStream::new(BufReader::new(File::open(&log_path).await?).lines()).try_fold(0, |acc, _| future::ok(acc + 1)).await?;
        let chaser = Chaser::new(log_path, chase::Line(init_lines));
        let stream = ReceiverStream::new(chaser.run())
            .scan(
                Arc::new(RwLock::new(FollowerState {
                    minecraft_version: None, //TODO check log history for current Minecraft version
                    death_messages: HashMap::default(),
                    http_client,
                })),
                |state, res| {
                    let state = Arc::clone(&state);
                    async move {
                        Some(match res {
                            Ok(line) => Line::parse(state, &line).await,
                            Err(e) => Err(e.into()),
                        })
                    }
                },
            );
        Ok::<_, Error>(stream)
    }).try_flatten()
}

pub async fn handle(ctx_fut: RwFuture<Context>) -> Result<Never, Error> { //TODO dynamically update handled worlds as they are added/removed
    let http_client = reqwest::Client::builder()
        .user_agent(concat!("wurstminebot/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .use_rustls_tls()
        .trust_dns(true)
        .https_only(true)
        .build()?;
    let mut handles = Vec::default();
    for world in World::all().await? {
        handles.push(tokio::spawn(handle_world(http_client.clone(), ctx_fut.clone(), world)));
    }
    match try_join_all(handles).await?.pop() {
        Some(Ok(never)) => match never {},
        Some(Err(e)) => return Err(e),
        None => {}
    }
    Err(Error::NoWorlds)
}

async fn handle_world(http_client: reqwest::Client, ctx_fut: RwFuture<Context>, world: World) -> Result<Never, Error> {
    let follower = follow(http_client, &world);
    pin_mut!(follower);
    while let Some(line) = follower.try_next().await? {
        match line {
            Line::Regular { content, .. } => match content {
                RegularLine::Chat { sender, msg, is_action } => {
                    let ctx = ctx_fut.read().await;
                    let ctx_data = (*ctx).data.read().await;
                    if let Some(chan_id) = ctx_data.get::<crate::config::Config>().expect("missing config").wurstminebot.world_channels.get(&world.to_string()) {
                        if let Ok(webhook) = chan_id.webhooks(&*ctx).await?.into_iter().exactly_one() {
                            webhook.execute(&*ctx, false, ExecuteWebhook::new()
                                .avatar_url(format!("https://minotar.net/armor/bust/{sender}/1024.png"))
                                .content(if is_action {
                                    let mut builder = MessageBuilder::default();
                                    builder.push_italic_safe(msg);
                                    builder.build()
                                } else {
                                    let mut builder = MessageBuilder::default();
                                    builder.push_safe(msg);
                                    builder.build()
                                })
                                .username(sender) //TODO use Discord nickname instead of Minecraft nickname?
                            ).await?;
                        }
                    }
                }
                RegularLine::Advancement { kind, player, advancement } => {
                    let ctx = ctx_fut.read().await;
                    let ctx_data = (*ctx).data.read().await;
                    if let Some(chan_id) = ctx_data.get::<crate::config::Config>().expect("missing config").wurstminebot.world_channels.get(&world.to_string()) {
                        chan_id.say(&*ctx, MessageBuilder::default()
                            .push_safe(player)
                            .push(match kind {
                                AdvancementKind::Challenge => " has completed the challenge [",
                                AdvancementKind::Goal => " has reached the goal [",
                                AdvancementKind::Task => " has made the advancement [",
                            })
                            .push_safe(advancement)
                            .push(']')
                            .build()).await?;
                    }
                }
                RegularLine::Death { msg, .. } => {
                    let ctx = ctx_fut.read().await;
                    let ctx_data = (*ctx).data.read().await;
                    if let Some(chan_id) = ctx_data.get::<crate::config::Config>().expect("missing config").wurstminebot.world_channels.get(&world.to_string()) {
                        chan_id.say(&*ctx, msg).await?;
                    }
                }
                RegularLine::ServerStart | RegularLine::Unknown(_) => {} // ignore all other lines for now
            },
            Line::Unknown(_) => {} // ignore all other lines for now
        }
    }
    Err(Error::FollowEnded)
}
