#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::{
    sync::LazyLock,
    time::{SystemTime, UNIX_EPOCH},
};

use async_std::net::UdpSocket;
use dark_light::Mode;
use discord_rich_presence::{
    DiscordIpc as _, DiscordIpcClient,
    activity::{Activity, Assets, Timestamps},
};
use iced::{
    Element, Subscription, Task, Theme,
    alignment::Vertical,
    futures::{SinkExt as _, Stream},
    stream,
    widget::{button, column, row, text, text_input},
    window,
};
use time::{OffsetDateTime, macros::format_description};
use tracing::{debug, info, level_filters::LevelFilter, warn};
use tracing_subscriber::EnvFilter;

#[derive(Clone, Debug)]
struct Telemetry {
    name: String,
    vehicle: String,
    ias: f64,
    alt_bar: f64,
    _t: u64,
}

// prettify some of the module names
static VEHICLE_NAMES: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "A-10C_2" => "A10-C",
    "F-16C_50" => "F-16CM bl.50",
};

static T_START: LazyLock<i64> = LazyLock::new(|| {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time marches ever on")
        .as_secs()
        .try_into()
        .expect("time doesn't fit in discord anymore")
});

struct State {
    drpc: DiscordIpcClient,
    connected: bool,
    last_update: Option<OffsetDateTime>,
    callsign: Option<String>,
}

#[derive(Clone, Debug)]
enum Message {
    CallsignChanged(String),
    ClientDisconnected,
    CloseRequested,
    Connect,
    TelemetryReceived(Telemetry),
}

impl State {
    fn new(drpc: DiscordIpcClient) -> Self {
        Self {
            drpc,
            connected: false,
            last_update: None,
            callsign: None,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Connect => {
                info!("connecting rpc");
                self.drpc.connect().unwrap();
                self.drpc.set_activity(idle_activity()).unwrap();
                self.connected = true;
            }
            Message::CallsignChanged(callsign) => {
                self.callsign = if callsign.is_empty() {
                    None
                } else {
                    Some(callsign)
                };
                info!(
                    self.callsign,
                    "callsign {}",
                    if self.callsign.is_some() {
                        "updated"
                    } else {
                        "cleared"
                    }
                );
            }
            Message::TelemetryReceived(telem) => {
                self.last_update = Some(OffsetDateTime::now_local().unwrap());
                let vehicle_pretty = VEHICLE_NAMES
                    .get(&telem.vehicle)
                    .map(|s| s.to_string())
                    .unwrap_or(telem.vehicle.clone());

                let name = self.callsign.as_deref().unwrap_or(&telem.name);
                info!(name, "telemetry received");

                let speed = if telem.ias > 10.0 {
                    telem.ias * 1.994
                } else {
                    0.0
                };

                let alt = (telem.alt_bar * 3.281) / 1000.0;

                self.drpc
                    .set_activity(
                        Activity::new()
                            .state(
                                &(if name == "New callsign" {
                                    format!("flying {vehicle_pretty}")
                                } else {
                                    format!("{name} in {vehicle_pretty}")
                                }),
                            )
                            .details(&format!("{speed:.0} knots at {alt:.0}k feet",))
                            .assets(
                                Assets::new()
                                    .small_image(&telem.vehicle.to_lowercase())
                                    .small_text(&vehicle_pretty),
                            )
                            .timestamps(Timestamps::new().start(*T_START)),
                    )
                    .unwrap();
            }
            Message::ClientDisconnected => {
                self.last_update = Some(OffsetDateTime::now_local().unwrap());
                info!("clean disconnect, returning to idle");
                self.drpc.set_activity(idle_activity()).unwrap();
            }
            Message::CloseRequested => {
                info!("closing discord connection");
                self.drpc.close().unwrap();
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        column![
            text(if self.connected {
                "Connected to discord."
            } else {
                "Connecting to discord..."
            }),
            text(if let Some(last_update) = self.last_update {
                format!(
                    "Last updated at {}",
                    last_update
                        .format(format_description!("[hour]:[minute]:[second]"))
                        .unwrap()
                )
            } else {
                "Waiting for telemetry...".to_string()
            }),
            row![
                text("Custom callsign"),
                row![
                    text_input("(in-game callsign)", self.callsign.as_deref().unwrap_or(""))
                        .on_input(Message::CallsignChanged),
                    button("X").on_press(Message::CallsignChanged(String::new()))
                ]
            ]
            .align_y(Vertical::Center)
            .spacing(6.0)
        ]
        .spacing(6.0)
        .padding(9.0)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::close_events().map(|_| Message::CloseRequested),
            Subscription::run(Self::socket_listener),
        ])
    }

    fn socket_listener() -> impl Stream<Item = Message> {
        stream::channel(32, |mut sender| async move {
            info!("starting telemetry listener");
            let socket = UdpSocket::bind("0.0.0.0:14242").await.unwrap();
            info!(addr = ?socket.local_addr().unwrap(), "ready to receive!");

            // big enough for any valid udp message
            let mut buf: Box<[u8; 65527]> = Box::new([0; 65527]);
            loop {
                let (len, src_addr) = socket.recv_from(buf.as_mut_slice()).await.unwrap();
                info!(
                    src_addr = src_addr.ip().to_string(),
                    len, "received datagram"
                );

                let line = str::from_utf8(&buf[..len]).unwrap();
                debug!(?line, "decoded line");
                if line == "bye" {
                    sender.send(Message::ClientDisconnected).await.unwrap();
                    continue;
                }

                let Some((cmd, rest)) = line.split_once(' ') else {
                    warn!(line, "ignoring improperly formatted line");
                    continue;
                };
                if cmd == "telem" {
                    let parts: Vec<_> = rest.split(',').collect();
                    let telem = Telemetry {
                        name: parts[0].to_string(),
                        vehicle: parts[1].to_string(),
                        ias: parts[2].parse().unwrap(),
                        alt_bar: parts[3].parse().unwrap(),
                        _t: parts[4].parse().unwrap(),
                    };
                    sender
                        .send(Message::TelemetryReceived(telem))
                        .await
                        .unwrap();
                }
            }
        })
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::ERROR.into())
                .from_env()?
                .add_directive(
                    format!(
                        "dcs_rich_presence={}",
                        if cfg!(debug_assertions) {
                            "debug"
                        } else {
                            "info"
                        }
                    )
                    .parse()?,
                ),
        )
        .init();

    iced::application("DCS Rich Presence", State::update, State::view)
        .subscription(State::subscription)
        .window_size((320.0, 100.0))
        .resizable(false)
        .theme(|_| match dark_light::detect().unwrap() {
            Mode::Dark => Theme::CatppuccinMocha,
            Mode::Light | Mode::Unspecified => Theme::CatppuccinLatte,
        })
        .run_with(|| {
            (
                State::new(DiscordIpcClient::new("1392523475775655936")),
                Task::done(Message::Connect),
            )
        })?;

    info!("bye");

    Ok(())
}

fn idle_activity<'a>() -> Activity<'a> {
    Activity::new()
        .state("Mission planning")
        .timestamps(Timestamps::new().start(*T_START))
}
