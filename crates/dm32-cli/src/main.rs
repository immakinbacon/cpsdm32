//! `dm32` — command-line programmer for the Baofeng DM-32UV.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use dm32_codeplug::radio::{self, Ctx, Progress, ReadOptions, WriteOptions};
use dm32_codeplug::{Image, block};
use dm32_proto::{
    LineConfig, LineLevel, SerialTransport, Session, Timing, TracingTransport, Transport,
};

#[derive(Parser)]
#[command(
    name = "dm32",
    version,
    about = "Program the Baofeng DM-32UV from Linux"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List serial ports.
    Ports,
    /// Identify the radio (handshake only, no programming mode).
    Probe(Conn),
    /// Read the codeplug from the radio into a file (.dm32 or .data).
    Read {
        #[command(flatten)]
        conn: Conn,
        /// Output file.
        #[arg(short, long)]
        output: PathBuf,
        /// Also read the full DMR contact database.
        #[arg(long)]
        contacts: bool,
    },
    /// Write a codeplug file to the radio. Only changed blocks are written; the radio's
    /// previous contents are saved as a backup first.
    Write {
        #[command(flatten)]
        conn: Conn,
        /// Codeplug file (.dm32 or .data).
        input: PathBuf,
        /// Write every block, not only the changed ones.
        #[arg(long)]
        all: bool,
        /// Never write the radio settings block.
        #[arg(long)]
        skip_settings: bool,
        /// Also write the DMR contact database.
        #[arg(long)]
        contacts: bool,
        /// Skip the read-back check.
        #[arg(long)]
        no_verify: bool,
        /// Where to save the backup (default: dm32-backup-<time>.dm32).
        #[arg(long)]
        backup: Option<PathBuf>,
        /// Show what would be written and stop before writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Show what a codeplug file contains (blocks).
    Info { file: PathBuf },
    /// Print the decoded codeplug: channels, zones, contacts, lists.
    Show { file: PathBuf },
    /// Compare two codeplug files block by block.
    Diff { a: PathBuf, b: PathBuf },
    /// Convert between .dm32 and the official CPS .data format.
    Convert { input: PathBuf, output: PathBuf },
}

#[derive(Args)]
struct Conn {
    /// Serial port (default: the only USB serial port present).
    #[arg(short, long)]
    port: Option<String>,
    /// RTS level while connected: high, low or default.
    #[arg(long, default_value = "default")]
    rts: LineLevel,
    /// DTR level while connected: high, low or default.
    #[arg(long, default_value = "default")]
    dtr: LineLevel,
    /// Log all serial traffic to this file (attach it to bug reports).
    #[arg(long)]
    trace: Option<PathBuf>,
    /// Skip the official CPS's optional extra requests.
    #[arg(long)]
    minimal: bool,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Ports => {
            let ports = dm32_proto::list_ports();
            if ports.is_empty() {
                println!("No USB serial ports found.");
            }
            for p in ports {
                println!("{p}");
            }
        }
        Cmd::Probe(conn) => {
            let mut s = connect(&conn)?;
            let info = s.identify()?.clone();
            println!("Model:            {}", info.model);
            println!("Firmware:         {}", info.firmware);
            println!("Build date:       {}", info.build_date);
            println!("DSP version:      {}", info.dsp_version);
            println!("Radio version:    {}", info.radio_version);
            println!("Codeplug version: {}", info.codeplug_version);
            if let Some(r) = info.config {
                println!("Config memory:    {:#08x}-{:#08x}", r.start, r.end);
            }
            if let Some(r) = info.contacts {
                println!("Contact memory:   {:#08x}-{:#08x}", r.start, r.end);
            }
            if let Some(n) = info.max_contacts {
                println!("Max contacts:     {n}");
            }
            for (id, v) in &info.vframes {
                println!(
                    "  V-frame {id:#04x}: {}",
                    v.iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
            disconnect(s);
        }
        Cmd::Read {
            conn,
            output,
            contacts,
        } => {
            let mut s = connect(&conn)?;
            let opts = ReadOptions {
                contacts,
                oem_compat: !conn.minimal,
            };
            let img = with_ctx(|cx| radio::read(&mut s, opts, cx));
            disconnect(s);
            let img = img?;
            img.save(&output)
                .with_context(|| format!("saving {}", output.display()))?;
            println!(
                "Read {} blocks, {} contact pages → {}",
                img.blocks.len(),
                img.contacts.len(),
                output.display()
            );
        }
        Cmd::Write {
            conn,
            input,
            all,
            skip_settings,
            contacts,
            no_verify,
            backup,
            dry_run,
        } => {
            let target =
                Image::load(&input).with_context(|| format!("loading {}", input.display()))?;
            let opts = WriteOptions {
                all_blocks: all,
                skip_settings,
                contacts,
                verify: !no_verify,
            };
            let backup = backup
                .unwrap_or_else(|| PathBuf::from(format!("dm32-backup-{}.dm32", timestamp())));
            let mut s = connect(&conn)?;
            let prepared = match with_ctx(|cx| radio::prepare(&mut s, &target, opts, cx)) {
                Ok(p) => p,
                Err(e) => {
                    disconnect(s);
                    return Err(e);
                }
            };
            if dry_run {
                disconnect(s);
                println!("{} pages would be written:", prepared.writes.len());
                for w in &prepared.writes {
                    let addr = if w.addr == radio::SENTINEL {
                        "new page".to_string()
                    } else {
                        format!("{:#08x}", w.addr)
                    };
                    println!("  {:<30} → {addr}", w.what);
                }
                return Ok(());
            }
            // Save the backup before touching the radio.
            if let Err(e) = prepared.before.save(&backup) {
                disconnect(s);
                return Err(anyhow::anyhow!(e).context(format!(
                    "saving backup {}; nothing was written",
                    backup.display()
                )));
            }
            println!("Backup of current radio contents: {}", backup.display());
            if prepared.writes.is_empty() {
                disconnect(s);
                println!(
                    "Radio already matches {}; nothing to write.",
                    input.display()
                );
                return Ok(());
            }
            let report = with_ctx(|cx| radio::execute(&mut s, &prepared, opts, cx));
            disconnect(s);
            let report = report?;
            println!("Wrote {} pages:", report.written.len());
            for w in &report.written {
                println!("  {w}");
            }
            if !report.mismatches.is_empty() {
                for (what, off) in &report.mismatches {
                    eprintln!("VERIFY FAILED: {what} differs at offset {off:#05x}");
                }
                bail!("verification failed for {} pages", report.mismatches.len());
            } else if !no_verify {
                println!("Verified OK.");
            }
        }
        Cmd::Info { file } => {
            let img = Image::load(&file)?;
            println!("Model:    {}", img.info.model);
            println!("Firmware: {}", img.info.firmware);
            println!("Blocks:");
            for (id, p) in &img.blocks {
                let used = p[..p.len() - 1]
                    .iter()
                    .filter(|&&b| b != 0xFF && b != 0)
                    .count();
                println!(
                    "  {id:#04x} {:<28} {used:>5} non-blank bytes",
                    block::name(*id)
                );
            }
            println!("Contact pages: {}", img.contacts.len());
            if let Some(b) = &img.boot_image {
                println!("Boot image: {} bytes", b.len());
            }
        }
        Cmd::Show { file } => show(&Image::load(&file)?),
        Cmd::Diff { a, b } => {
            let (ia, ib) = (Image::load(&a)?, Image::load(&b)?);
            let mut n = 0;
            let ids: std::collections::BTreeSet<u8> =
                ia.blocks.keys().chain(ib.blocks.keys()).copied().collect();
            for id in ids {
                match (ia.blocks.get(&id), ib.blocks.get(&id)) {
                    (Some(x), Some(y)) if x == y => {}
                    (Some(x), Some(y)) => {
                        let offs: Vec<usize> = (0..x.len()).filter(|&i| x[i] != y[i]).collect();
                        println!("{id:#04x} {}: {} bytes differ", block::name(id), offs.len());
                        for &o in offs.iter().take(16) {
                            println!("    {o:#05x}: {:02x} → {:02x}", x[o], y[o]);
                        }
                        n += 1;
                    }
                    (Some(_), None) => {
                        println!("{id:#04x} {}: only in {}", block::name(id), a.display());
                        n += 1;
                    }
                    (None, Some(_)) => {
                        println!("{id:#04x} {}: only in {}", block::name(id), b.display());
                        n += 1;
                    }
                    (None, None) => {}
                }
            }
            let contacts_differ = ia.contacts != ib.contacts;
            if contacts_differ {
                println!("contact database differs");
            }
            if n == 0 && !contacts_differ {
                println!("No differences.");
            }
        }
        Cmd::Convert { input, output } => {
            let img = Image::load(&input)?;
            img.save(&output)?;
            println!("{} → {}", input.display(), output.display());
        }
    }
    Ok(())
}

type RadioSession = Session<Box<dyn Transport>>;

fn connect(conn: &Conn) -> Result<RadioSession> {
    let port = match &conn.port {
        Some(p) => p.clone(),
        None => {
            let ports = dm32_proto::list_ports();
            match ports.as_slice() {
                [p] => p.clone(),
                [] => {
                    bail!("no USB serial port found; plug in the programming cable or use --port")
                }
                _ => bail!(
                    "several serial ports found ({}); choose one with --port",
                    ports.join(", ")
                ),
            }
        }
    };
    let lines = LineConfig {
        rts: conn.rts,
        dtr: conn.dtr,
    };
    let serial = SerialTransport::open(&port, lines).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("ermission denied") {
            anyhow::anyhow!("{msg}\nhint: add your user to the 'dialout' group (sudo usermod -aG dialout $USER), then log in again")
        } else {
            anyhow::anyhow!(msg)
        }
    })?;
    eprintln!("Connected to {port}");
    let t: Box<dyn Transport> = match &conn.trace {
        Some(path) => {
            let f = File::create(path).with_context(|| format!("creating {}", path.display()))?;
            Box::new(TracingTransport::new(serial, Box::new(f)))
        }
        None => Box::new(serial),
    };
    let mut s = Session::new(t, Timing::hardware());
    s.set_oem_compat(!conn.minimal);
    Ok(s)
}

fn disconnect(s: RadioSession) {
    // Dropping the port closes it, which resets the radio out of programming mode.
    drop(s);
}

fn with_ctx<T>(f: impl FnOnce(&mut Ctx) -> dm32_proto::Result<T>) -> Result<T> {
    let cancel = AtomicBool::new(false);
    let mut stage = String::new();
    let mut progress = |p: Progress| match p {
        Progress::Stage(s) => {
            if !stage.is_empty() {
                eprintln!();
            }
            eprint!("{s}...");
            stage = s;
        }
        Progress::Step { done, total } => {
            eprint!("\r{stage}... {done}/{total}  ");
            let _ = std::io::stderr().flush();
        }
    };
    let r = f(&mut Ctx {
        progress: &mut progress,
        cancel: &cancel,
    });
    eprintln!();
    r.map_err(|e| anyhow::anyhow!(e).context(hint_for_error()))
}

fn hint_for_error() -> &'static str {
    "radio communication failed (if the radio was already used in this power cycle, switch it off and on, then retry; \
     some firmware needs --rts high --dtr low; run with --trace trace.log and keep the log)"
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn show(img: &Image) {
    use dm32_codeplug::Codeplug;
    use dm32_codeplug::records::Choice;
    let cp = Codeplug::decode(img);
    println!("Channels ({}):", cp.channels.list.len());
    for (i, e) in cp.channels.list.iter().enumerate() {
        if e.is_empty() {
            println!("{:>5}  (empty)", i + 1);
            continue;
        }
        let tx = e.tx_freq().map_or("-".into(), |f| f.to_string());
        let extra = if e.mode().is_digital() {
            let tg = e
                .tx_contact
                .index()
                .and_then(|t| cp.talkgroups.slots.get(t as usize)?.as_ref())
                .map(|t| t.name());
            format!(
                "CC{} {} {}",
                e.color_code().unwrap_or(0),
                e.time_slot().map_or(String::new(), |t| t.label()),
                tg.unwrap_or_default()
            )
        } else {
            format!("{} / {}", e.rx_tone(), e.tx_tone())
        };
        println!(
            "{:>5}  {:<16} {:<8} {} {:>10} {:<6} {extra}",
            i + 1,
            e.name(),
            e.mode().label(),
            e.rx_freq(),
            tx,
            e.power().label()
        );
    }
    println!("\nZones ({}):", cp.zones.zones.len());
    for z in &cp.zones.zones {
        println!("  {:<16} {:?}", z.name(), z.channels());
    }
    println!("\nScan lists ({}):", cp.scan_lists.lists.len());
    for l in &cp.scan_lists.lists {
        println!("  {:<16} {:?}", l.name(), l.members());
    }
    println!("\nDigital contacts:");
    for (i, t) in cp.talkgroups.iter() {
        println!(
            "{i:>5}  {:<16} {:>8} {}",
            t.name(),
            t.id(),
            t.call_type().label()
        );
    }
    println!("\nRX group lists:");
    for (i, g) in cp.rx_groups.iter() {
        println!("{:>5}  {:<16} {:?}", i, g.name(), g.ids());
    }
    println!("\nRadio IDs:");
    for r in &cp.radio_ids.ids {
        println!("  {:<16} {}", r.name(), r.id());
    }
    println!("\nQuick texts:");
    for t in &cp.quick_texts.messages {
        println!("  {}", t.text());
    }
    if let Some(s) = &cp.settings {
        println!(
            "\nPower-on lines: {:?} / {:?}",
            s.power_on_line1(),
            s.power_on_line2()
        );
    }
    let problems = cp.problems();
    if !problems.is_empty() {
        println!("\nProblems:");
        for p in problems {
            println!("  {p}");
        }
    }
}
