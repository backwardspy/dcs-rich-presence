use std::{
    io::{BufRead as _, BufReader},
    net::TcpListener,
    time::{SystemTime, UNIX_EPOCH},
};

use discord_rich_presence::{
    DiscordIpc as _, DiscordIpcClient,
    activity::{Activity, Timestamps},
};
use tracing::{info, warn};

#[derive(Debug)]
struct Telemetry<'a> {
    name: &'a str,
    vehicle: &'a str,
    ias: f64,
    alt_bar: f64,
    t: u64,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut drpc = DiscordIpcClient::new("1392523475775655936");
    drpc.connect()?;
    drpc.set_activity(idle_activity())?;

    let listener = TcpListener::bind("0.0.0.0:14242")?;
    info!(addr = ?listener.local_addr()?, "listening");
    for stream in listener.incoming() {
        let stream = stream?;
        info!(addr = ?stream.peer_addr()?, "client connected!");

        let reader = BufReader::new(&stream);
        for line in reader.lines() {
            let line = line?;
            if line == "bye" {
                info!("clean disconnect");
                break;
            }

            let Some((cmd, rest)) = line.split_once(' ') else {
                warn!(line, "ignoring improperly formatted line");
                break;
            };
            if cmd == "telem" {
                let parts: Vec<_> = rest.split(',').collect();
                let telem = Telemetry {
                    name: parts[0],
                    vehicle: parts[1],
                    ias: parts[2].parse()?,
                    alt_bar: parts[3].parse()?,
                    t: parts[4].parse()?,
                };
                drpc.set_activity(
                    Activity::new()
                        .state(&format!("{} in {}", telem.name, telem.vehicle))
                        .details(&format!(
                            "{:.0} kts @ {:.0}k ft",
                            telem.ias * 1.944,
                            (telem.alt_bar * 3.281) / 1000.0
                        ))
                        .timestamps(
                            Timestamps::new().start(
                                (SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                    - telem.t) as i64,
                            ),
                        ),
                )?;
            }
        }
        info!("client disconnected");
        drpc.set_activity(idle_activity())?;
    }

    drpc.close()?;

    Ok(())
}

fn idle_activity<'a>() -> Activity<'a> {
    Activity::new().state("Mission planning")
}
