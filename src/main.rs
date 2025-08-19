use std::{
    io,
    net::UdpSocket,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use discord_rich_presence::{
    DiscordIpc as _, DiscordIpcClient,
    activity::{Activity, Assets, Timestamps},
};
use tracing::{Level, debug, info, warn};

#[derive(Debug)]
struct Telemetry<'a> {
    name: &'a str,
    vehicle: &'a str,
    ias: f64,
    alt_bar: f64,
    _t: u64,
}

// prettify some of the module names
static VEHICLE_NAMES: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "A-10C_2" => "A10-C II",
    "F-16C_50" => "F-16C bl.50",
};

static T_START: LazyLock<i64> = LazyLock::new(|| {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time marches ever on")
        .as_secs()
        .try_into()
        .expect("time doesn't fit in discord anymore")
});

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(if cfg!(debug_assertions) {
            Level::DEBUG
        } else {
            Level::INFO
        })
        .init();

    let exit_requested = Arc::new(AtomicBool::new(false));
    let exit_requested_ctrlc = exit_requested.clone();
    ctrlc::set_handler(move || {
        info!("exit requested, give me a second to clean up...");
        exit_requested_ctrlc.store(true, Ordering::SeqCst);
    })
    .expect("can set sigint handler");

    info!("connecting rpc");
    let mut drpc = DiscordIpcClient::new("1392523475775655936");
    drpc.connect()?;
    drpc.set_activity(idle_activity())?;

    info!("starting telemetry listener");
    let socket = UdpSocket::bind("0.0.0.0:14242")?;
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    info!(addr = ?socket.local_addr()?, "ready to receive!");

    // big enough for any valid udp message
    let mut buf: Box<[u8; 65527]> = Box::new([0; 65527]);
    while !exit_requested.load(Ordering::SeqCst) {
        let (len, src_addr) = match socket.recv_from(buf.as_mut_slice()) {
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                continue;
            }
            r => r?,
        };
        info!(
            src_addr = src_addr.ip().to_string(),
            len, "received datagram"
        );

        let line = str::from_utf8(&buf[..len])?;
        debug!(?line, "decoded line");
        if line == "bye" {
            info!("clean disconnect, returning to idle");
            drpc.set_activity(idle_activity())?;
            continue;
        }

        let Some((cmd, rest)) = line.split_once(' ') else {
            warn!(line, "ignoring improperly formatted line");
            continue;
        };
        if cmd == "telem" {
            let parts: Vec<_> = rest.split(',').collect();
            let telem = Telemetry {
                name: parts[0],
                vehicle: parts[1],
                ias: parts[2].parse()?,
                alt_bar: parts[3].parse()?,
                _t: parts[4].parse()?,
            };
            let vehicle_pretty = VEHICLE_NAMES.get(telem.vehicle).unwrap_or(&telem.vehicle);
            drpc.set_activity(
                Activity::new()
                    .state(
                        &(if telem.name == "New callsign" {
                            format!("flying {vehicle_pretty}")
                        } else {
                            format!("{} in {}", telem.name, vehicle_pretty)
                        }),
                    )
                    .details(&format!(
                        "{:.0} kts @ {:.0}k ft",
                        telem.ias * 1.944,
                        (telem.alt_bar * 3.281) / 1000.0
                    ))
                    .assets(
                        Assets::new()
                            .small_image(&telem.vehicle.to_lowercase())
                            .small_text(vehicle_pretty),
                    )
                    .timestamps(Timestamps::new().start(*T_START)),
            )?;
        }
    }

    info!("closing rpc");
    drpc.close()?;

    info!("bye");

    Ok(())
}

fn idle_activity<'a>() -> Activity<'a> {
    Activity::new()
        .state("Mission planning")
        .timestamps(Timestamps::new().start(*T_START))
}
