//! Build, package and test tooling: `cargo xtask <command>`.
//!
//! ```text
//! build [--release]          tsx, tsfun and hamsandwich for i686 and x86_64, Linux and Windows
//! dist [--debug-info]        release build -> dist/<platform>/addons/amxmodx/{modules,scripting/include}/ (stripped
//!                            unless --debug-info), with AMX Mod X's file names
//! lint                       cargo fmt --check, clippy -D warnings on every target
//! test                       unit tests on x86_64 and i686 Linux (each runs its ABI layout tests)
//! e2e [linux] [win] [linux64]  modules + e2e/*.sma (tsx, hamsandwich, tsbot) on the amxx-testenv roots, a bot match on
//!                            ts_dojo, then the plugin's checks (default: linux)
//! ```
//!
//! Environment: `FWGS_DIR` (FWGS headers, default: `xash3d-fwgs` next to the checkout or one of its
//! ancestors, so git worktrees under `.claude/worktrees/` find it too; an `FWGS_DIR` without the headers is ignored),
//! `AMXX_TESTENV` (default `~/dev/hl/ref/amxx-testenv`), `AMXX_SCRIPTING` (amxxpc's directory),
//! `AMXX_LINUX_ROOT` / `AMXX_WIN_ROOT` / `AMXX_LINUX64_ROOT` (default `~/games/amxx_rehlds_{linux,win,linux64}`),
//! `AMXX_TSSTATS_SMA` (upstream's score plugin source), `E2E_TIMEOUT` (seconds to wait for the plugin, default
//! 150), `E2E_NO_STATS` (skip the stats round trip), `E2E_NO_TSBOT` (skip tsbot.inc's phase), `E2E_ONLY_TSBOT` (only it).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

/// A build target and the file names AMX Mod X loads.
struct Target {
    triple: &'static str,
    platform: &'static str,
    /// (cargo output file, AMX Mod X module file).
    files: [(&'static str, &'static str); 3],
    strip: &'static str,
}

/// The 64-bit ones are for goldsrc-net's AMX Mod X fork (its Linux modules end in `_amd64.so`;
/// on Windows the names stay the 32-bit build's).
const TARGETS: [Target; 4] = [
    Target {
        triple: "i686-unknown-linux-gnu",
        platform: "linux-i686",
        files: [
            ("libtsx.so", "tsx_amxx_i386.so"),
            ("libtsfun.so", "tsfun_amxx_i386.so"),
            ("libhamsandwich.so", "hamsandwich_amxx_i386.so"),
        ],
        strip: "strip",
    },
    Target {
        triple: "i686-pc-windows-gnu",
        platform: "windows-i686",
        files: [("tsx.dll", "tsx_amxx.dll"), ("tsfun.dll", "tsfun_amxx.dll"), ("hamsandwich.dll", "hamsandwich_amxx.dll")],
        strip: "i686-w64-mingw32-strip",
    },
    Target {
        triple: "x86_64-unknown-linux-gnu",
        platform: "linux-x86_64",
        files: [
            ("libtsx.so", "tsx_amxx_amd64.so"),
            ("libtsfun.so", "tsfun_amxx_amd64.so"),
            ("libhamsandwich.so", "hamsandwich_amxx_amd64.so"),
        ],
        strip: "strip",
    },
    Target {
        triple: "x86_64-pc-windows-gnu",
        platform: "windows-x86_64",
        files: [("tsx.dll", "tsx_amxx.dll"), ("tsfun.dll", "tsfun_amxx.dll"), ("hamsandwich.dll", "hamsandwich_amxx.dll")],
        strip: "x86_64-w64-mingw32-strip",
    },
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

fn env_path(var: &str, default: PathBuf) -> PathBuf {
    std::env::var(var).map(PathBuf::from).unwrap_or(default)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    eprintln!("$ {cmd:?}");
    let st = cmd.status().map_err(|e| format!("{cmd:?}: {e}"))?;
    if st.success() { Ok(()) } else { Err(format!("{cmd:?} failed: {st}")) }
}

fn is_fwgs(dir: &Path) -> bool {
    dir.join("common/xash3d_types.h").is_file()
}

/// `xash3d-fwgs` next to the checkout or the nearest ancestor that has one. `.cargo/config.toml`'s
/// fixed `../xash3d-fwgs` misses it from a worktree nested in the checkout, and cargo hands that
/// value to xtask too, so `FWGS_DIR` only wins when it holds the FWGS headers.
fn fwgs_dir() -> Option<PathBuf> {
    root().ancestors().skip(1).map(|d| d.join("xash3d-fwgs")).find(|p| is_fwgs(p))
}

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    if !std::env::var_os("FWGS_DIR").is_some_and(|d| is_fwgs(Path::new(&d)))
        && let Some(dir) = fwgs_dir()
    {
        c.env("FWGS_DIR", dir);
    }
    c
}

fn build(release: bool) -> Result<(), String> {
    for t in &TARGETS {
        let mut c = cargo();
        c.args(["build", "-p", "tsx", "-p", "tsfun", "-p", "hamsandwich", "--target", t.triple]);
        if release {
            c.arg("--release");
        }
        run(&mut c)?;
    }
    Ok(())
}

fn built(t: &Target, release: bool, cargo_name: &str) -> PathBuf {
    root().join("target").join(t.triple).join(if release { "release" } else { "debug" }).join(cargo_name)
}

/// The server loads these DLLs with only system DLLs around: no MinGW runtime imports.
fn check_windows_dll(t: &Target, dll: &Path, metamod: bool) -> Result<(), String> {
    let objdump = t.strip.replace("strip", "objdump");
    let out = Command::new(&objdump).arg("-p").arg(dll).output().map_err(|e| format!("{objdump}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    for bad in ["libgcc", "libwinpthread", "libstdc"] {
        if text.lines().any(|l| l.contains("dll name:") && l.contains(bad)) {
            return Err(format!("{} imports a MinGW runtime DLL ({bad})", dll.display()));
        }
    }
    // tsx is a metamod plugin too: metamod looks up the undecorated name (stdcall on win32).
    if metamod && (!text.contains("givefnptrstodll") || text.contains("givefnptrstodll@")) {
        return Err(format!("{} doesn't export an undecorated GiveFnptrsToDll", dll.display()));
    }
    Ok(())
}

fn dist(debug_info: bool) -> Result<(), String> {
    build(true)?;
    for t in &TARGETS {
        let dir = root().join("dist").join(t.platform).join("addons/amxmodx/modules");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (from, to) in t.files {
            let dest = dir.join(to);
            std::fs::copy(built(t, true, from), &dest).map_err(|e| format!("{from}: {e}"))?;
            if !debug_info {
                run(Command::new(t.strip).arg(&dest))?;
            }
            if t.platform.starts_with("windows") {
                // tsx and hamsandwich are metamod plugins too.
                check_windows_dll(t, &dest, !to.starts_with("tsfun"))?;
            }
            eprintln!("  {}", dest.display());
        }
        // The patched includes, for recompiling plugins: tsxaddon.inc (ts_setuserammo as a
        // native), tsconst.inc (TS 3.0's weapon ids), tsfun.inc (the forwards tsx fires) and the
        // port addition tsbot.inc (tsx's bot natives and forwards).
        let inc = root().join("dist").join(t.platform).join("addons/amxmodx/scripting/include");
        std::fs::create_dir_all(&inc).map_err(|e| e.to_string())?;
        for f in ["tsxaddon.inc", "tsconst.inc", "tsfun.inc", "tsbot.inc"] {
            std::fs::copy(root().join("include").join(f), inc.join(f)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn lint() -> Result<(), String> {
    run(cargo().args(["fmt", "--all", "--check"]))?;
    for t in &TARGETS {
        run(cargo().args(["clippy", "--workspace", "--all-targets", "--target", t.triple, "--", "-D", "warnings"]))?;
    }
    Ok(())
}

fn test() -> Result<(), String> {
    run(cargo().args(["test", "--workspace", "--target", "x86_64-unknown-linux-gnu"]))?;
    run(cargo().args(["test", "--workspace", "--target", "i686-unknown-linux-gnu"]))
}

// ---- e2e ----

struct Root {
    os: &'static str,
    dir: PathBuf,
    port: u16,
    target: &'static Target,
    exe: &'static str,
}

fn roots(names: &[String]) -> Result<Vec<Root>, String> {
    let names: Vec<&str> = if names.is_empty() { vec!["linux"] } else { names.iter().map(String::as_str).collect() };
    names
        .into_iter()
        .map(|n| match n {
            "linux" => Ok(Root {
                os: "linux",
                dir: env_path("AMXX_LINUX_ROOT", home().join("games/amxx_rehlds_linux")),
                port: 27315,
                target: &TARGETS[0],
                exe: "hlds_linux",
            }),
            "win" => Ok(Root {
                os: "win",
                dir: env_path("AMXX_WIN_ROOT", home().join("games/amxx_rehlds_win")),
                port: 27316,
                target: &TARGETS[1],
                exe: "hlds.exe",
            }),
            "linux64" => Ok(Root {
                os: "linux64",
                dir: env_path("AMXX_LINUX64_ROOT", home().join("games/amxx_rehlds_linux64")),
                port: 27317,
                target: &TARGETS[2],
                exe: "hlds_linux",
            }),
            other => Err(format!("unknown e2e root {other} (linux|win|linux64)")),
        })
        .collect()
}

/// A server of this root is running (same check as the testenv's stop.sh: name and cwd).
fn running(r: &Root) -> bool {
    let Ok(out) = Command::new("pgrep").args(["-x", r.exe]).output() else { return false };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|p| std::fs::read_link(format!("/proc/{}/cwd", p.trim())).ok())
        .any(|cwd| cwd == r.dir)
}

/// Adds `line` to a config file unless a line with the same first word is there.
fn ensure_line(file: &Path, line: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let key = line.split_whitespace().next().unwrap_or(line);
    if text.lines().any(|l| l.split_whitespace().next() == Some(key)) {
        return Ok(());
    }
    let sep = if text.ends_with('\n') || text.is_empty() { "" } else { "\n" };
    std::fs::write(file, format!("{text}{sep}; tsmod-amxx-rs e2e\n{line}\n")).map_err(|e| e.to_string())
}

/// The end-to-end plugins: `e2e/<name>.sma`, each logging to `logs/<name>.log`.
const PLUGINS: [&str; 2] = ["tsx_e2e", "ham_e2e"];

/// tsbot.inc's test plugin: a bot backend of its own, on a server of its own without quota bots
/// (it takes the bots over, which the match above needs), after the match ([`tsbot_phase`]).
const BOT_PLUGIN: &str = "tsbot_e2e";

/// The modules `amxx modules` must list as running.
const MODULE_NAMES: [&str; 3] = ["TSX", "TSFun Wrapper", "Ham Sandwich"];

/// Native errors the e2e plugins cause on purpose: a hook on an unknown class, and ham_e2e's
/// `ExecuteHamB(Ham_Use)` that the server refuses (BUSY) at its hook nesting limit.
const EXPECTED_ERRORS: [&str; 2] = ["no_such_class_e2e", "; use didn't run."];

/// The stats round trip's score plugins (`data/tsstats.amxx`): upstream's `tsstats.sma` for the
/// match, and `e2e/tsstats_score_e2e.sma` (deaths - kills, the reverse order) for the reload.
const SCORE_STOCK: &str = "tsstats";
const SCORE_E2E: &str = "tsstats_score_e2e";

fn compile_plugin(scripting: &Path, name: &str, out: &Path) -> Result<(), String> {
    let sma = if name == SCORE_STOCK {
        env_path("AMXX_TSSTATS_SMA", home().join("dev/hl/ref/amxmodx/plugins/ts/tsstats.sma"))
    } else {
        root().join(format!("e2e/{name}.sma"))
    };
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    // AMX Mod X 1.8.2's amxxpc exits 0 after errors: only a fresh output file means success.
    let _ = std::fs::remove_file(out);
    let mut c = Command::new(scripting.join("amxxpc"));
    // The repo's include/ first: its tsxaddon.inc declares the ts_setuserammo native, its
    // tsconst.inc has TS 3.0's weapon ids and its tsfun.inc the forwards tsx fires.
    c.env("LD_LIBRARY_PATH", scripting)
        .arg(&sma)
        .arg(format!("-i{}", root().join("include").display()))
        .arg(format!("-i{}", scripting.join("include").display()))
        .arg(format!("-o{}", out.display()));
    // amxxpc writes `<out>.amx` scratch files next to the current directory.
    c.current_dir(out.parent().unwrap());
    let o = c.output().map_err(|e| format!("amxxpc: {e}"))?;
    if !o.status.success() || !out.exists() {
        return Err(format!(
            "amxxpc failed on {name} ({}):\n{}{}",
            o.status,
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ));
    }
    Ok(())
}

fn plugin_path(name: &str) -> PathBuf {
    root().join(format!("target/e2e/{name}.amxx"))
}

/// Installs the modules (hamsandwich under the stock module's file name, replacing it) and the
/// plugins into a root that `amxx-testenv/setup.sh` built; setup.sh rebuilds roots from scratch,
/// so this runs before every e2e run.
fn install(r: &Root) -> Result<(), String> {
    let amxx = r.dir.join("ts/addons/amxmodx");
    if !amxx.join("configs/modules.ini").exists() {
        return Err(format!("{} has no AMX Mod X: run amxx-testenv/setup.sh {}", r.dir.display(), r.os));
    }
    for (from, to) in r.target.files {
        let dest = amxx.join("modules").join(to);
        let _ = std::fs::remove_file(&dest); // never write through a hardlink
        std::fs::copy(built(r.target, true, from), &dest).map_err(|e| format!("{from}: {e}"))?;
    }
    for m in ["tsx", "tsfun", "hamsandwich"] {
        ensure_line(&amxx.join("configs/modules.ini"), m)?;
    }
    for name in PLUGINS {
        let dest = amxx.join(format!("plugins/{name}.amxx"));
        let _ = std::fs::remove_file(&dest);
        std::fs::copy(plugin_path(name), &dest).map_err(|e| e.to_string())?;
        ensure_line(&amxx.join("configs/plugins.ini"), &format!("{name}.amxx debug"))?;
        let _ = std::fs::remove_file(amxx.join(format!("logs/{name}.log")));
    }
    // The stats round trip starts from no rank file, with upstream's score plugin.
    let _ = std::fs::remove_file(amxx.join("data/tsstats.dat"));
    put_score_plugin(r, SCORE_STOCK)?;
    Ok(())
}

/// Installs `target/e2e/<name>.amxx` as tsx's score plugin (`tsstats_score`'s default path).
fn put_score_plugin(r: &Root, name: &str) -> Result<(), String> {
    let dest = r.dir.join("ts/addons/amxmodx/data/tsstats.amxx");
    let _ = std::fs::remove_file(&dest);
    std::fs::copy(plugin_path(name), &dest).map(|_| ()).map_err(|e| format!("{}: {e}", dest.display()))
}

/// A `tsstats.dat` entry, or a `RANK` line of tsx_e2e's dump: the name, `get_stats`'s seven
/// counters (kills, deaths, headshots, team kills, shots, hits, damage) and body hits 1..=7.
#[derive(Debug, Clone, PartialEq)]
struct RankEntry {
    name: String,
    stats: [i32; 7],
    body: [i32; 7],
}

/// Reads `tsstats.dat` (tsx's `rank.rs` / upstream's `saveRank`): `i16 version (5)`, then
/// `i16 namelen; name; i16 uniquelen; unique; i32 kills, deaths, hs, tks, damage, hits, shots;
/// i32 body[8]` per entry, up to an `i16 0`.
fn read_rank_file(path: &Path) -> Result<Vec<RankEntry>, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut at = 0usize;
    let mut take = |n: usize| -> Result<&[u8], String> {
        let s = b.get(at..at + n).ok_or_else(|| format!("{}: short file at byte {at}", path.display()))?;
        at += n;
        Ok(s)
    };
    let short = |s: &[u8]| i16::from_le_bytes([s[0], s[1]]);
    let int = |s: &[u8]| i32::from_le_bytes([s[0], s[1], s[2], s[3]]);
    let version = short(take(2)?);
    if version != 5 {
        return Err(format!("{}: version {version}, want 5", path.display()));
    }
    let mut out = Vec::new();
    loop {
        let n = short(take(2)?);
        if n <= 0 {
            break;
        }
        let name = String::from_utf8_lossy(take(n as usize)?).trim_end_matches('\0').to_owned();
        let u = short(take(2)?);
        take(u.max(0) as usize)?;
        let mut c = [0i32; 7];
        for v in &mut c {
            *v = int(take(4)?);
        }
        let mut body = [0i32; 8];
        for v in &mut body {
            *v = int(take(4)?);
        }
        let [k, d, hs, tk, dmg, hits, shots] = c;
        out.push(RankEntry { name, stats: [k, d, hs, tk, shots, hits, dmg], body: body[1..].try_into().unwrap() });
    }
    Ok(out)
}

/// The `RANK <tag> <pos> <7 stats> <7 body> | <name>` lines tsx_e2e's `tsx_e2e_dump <tag>`
/// wrote, in rank order.
fn read_rank_dump(log: &str, tag: &str) -> Vec<RankEntry> {
    let prefix = format!("RANK {tag} ");
    log.lines()
        .filter_map(|l| l.split_once(": ").map(|(_, m)| m))
        .filter_map(|m| m.strip_prefix(&prefix))
        .filter_map(|m| {
            let (nums, name) = m.split_once(" | ")?;
            let v: Vec<i32> = nums.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
            (v.len() == 14).then(|| RankEntry {
                name: name.to_owned(),
                stats: v[..7].try_into().unwrap(),
                body: v[7..].try_into().unwrap(),
            })
        })
        .collect()
}

/// Waits until `cond` holds for the text of `file`.
fn wait_for(file: &Path, timeout: Duration, cond: impl Fn(&str) -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < timeout {
        if cond(&std::fs::read_to_string(file).unwrap_or_default()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

/// The engine console log (`-condebug`): `ts/` on Linux, the root on Windows (ReHLDS).
fn qconsole(r: &Root) -> PathBuf {
    if r.os.starts_with("linux") { r.dir.join("ts/qconsole.log") } else { r.dir.join("qconsole.log") }
}

fn file_len(p: &Path) -> usize {
    std::fs::read(p).map(|b| b.len()).unwrap_or(0)
}

/// The console text written since `from` bytes (the file may have been recreated: then all).
fn console_since(r: &Root, from: usize) -> String {
    let b = std::fs::read(qconsole(r)).unwrap_or_default();
    String::from_utf8_lossy(b.get(from..).unwrap_or(&b)).into_owned()
}

/// Stops the server with `quit` over rcon, so the engine runs `ServerDeactivate`; `stop.sh`
/// if it is still up after 20 s. Returns whether `quit` ended it.
fn quit(r: &Root, testenv: &Path) -> bool {
    rcon(testenv, r.port, "quit");
    let t0 = Instant::now();
    while running(r) && t0.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(500));
    }
    let clean = !running(r);
    let _ = Command::new(testenv.join("stop.sh")).arg(r.os).status();
    clean
}

/// The rank file round trip (#4), with the match's server still running: `changelevel` runs
/// `ServerDeactivate`, which saves `tsstats.dat`; the next map's tsx_e2e dumps the rank (still in
/// memory), which must equal the file, best first by upstream's `tsstats.amxx` score
/// (kills - deaths - team kills). Then `quit`, and a new server without bots and with the
/// reversing score plugin loads the file: its rank holds the same entries, now ordered by
/// deaths - kills, which only `get_score` from `data/tsstats.amxx` can do.
fn stats_round_trip(r: &Root, testenv: &Path, log: &Path, console_from: usize) -> Vec<String> {
    let os = r.os;
    let mut fails = Vec::new();
    let dat = r.dir.join("ts/addons/amxmodx/data/tsstats.dat");
    let starts = |t: &str| t.lines().filter(|l| l.contains(": start")).count();
    let before = starts(&std::fs::read_to_string(log).unwrap_or_default());
    let sorted_by = |v: &[RankEntry], score: fn(&RankEntry) -> i32| v.windows(2).all(|w| score(&w[0]) >= score(&w[1]));
    let stock_score: fn(&RankEntry) -> i32 = |e| e.stats[0] - e.stats[1] - e.stats[3];
    let reverse_score: fn(&RankEntry) -> i32 = |e| e.stats[1] - e.stats[0];

    rcon(testenv, r.port, "localinfo tsx_e2e_dump saved");
    rcon(testenv, r.port, "changelevel ts_dojo");
    if !wait_for(log, Duration::from_secs(60), |t| t.contains("DUMPED saved")) {
        fails.push(format!("{os}: stats: no rank dump after changelevel"));
        quit(r, testenv);
        return fails;
    }
    let dumped = read_rank_dump(&std::fs::read_to_string(log).unwrap_or_default(), "saved");
    match read_rank_file(&dat) {
        Err(e) => fails.push(format!("{os}: stats: changelevel saved no rank file: {e}")),
        Ok(file) => {
            println!(
                "  stats: changelevel saved {} entries ({} bytes); the rank in memory has {}",
                file.len(),
                file_len(&dat),
                dumped.len()
            );
            if file.is_empty() || file != dumped {
                fails.push(format!("{os}: stats: tsstats.dat after changelevel != get_stats:\n    file {file:?}\n    rank {dumped:?}"));
            }
            if !sorted_by(&file, stock_score) {
                fails.push(format!("{os}: stats: the saved rank isn't ordered by tsstats.amxx's kills - deaths - tks: {file:?}"));
            }
        }
    }
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let saved_at = mtime(&dat);
    let clean = quit(r, testenv);
    let resaved = mtime(&dat) != saved_at;
    println!(
        "  stats: quit {}; tsstats.dat {}",
        if clean { "ended the server" } else { "didn't end the server (stop.sh)" },
        if resaved { "saved again" } else { "not saved again" }
    );
    let console = console_since(r, console_from);
    for l in console.lines().filter(|l| l.contains("[TSX]")) {
        println!("  console: {l}");
        if l.contains("Couldn't load plugin") {
            fails.push(format!("{os}: stats: {l}"));
        }
    }
    let file = match read_rank_file(&dat) {
        Ok(f) => f,
        Err(e) => {
            fails.push(format!("{os}: stats: {e}"));
            return fails;
        }
    };

    // The reload: no bots, so the rank is the file's; the reversing score plugin.
    if let Err(e) = put_score_plugin(r, SCORE_E2E) {
        fails.push(format!("{os}: stats: {e}"));
        return fails;
    }
    let console_from = file_len(&qconsole(r));
    if let Err(e) = run(Command::new(testenv.join("run.sh")).args([os, "--map", "ts_dojo", "--bots", "0"])) {
        fails.push(format!("{os}: stats: {e}"));
        return fails;
    }
    if wait_for(log, Duration::from_secs(60), |t| starts(t) > before + 1) {
        rcon(testenv, r.port, "tsx_e2e_dump loaded");
    }
    let ok = wait_for(log, Duration::from_secs(20), |t| t.contains("DUMPED loaded"));
    quit(r, testenv);
    let _ = std::fs::remove_file(r.dir.join("ts/addons/amxmodx/data/tsstats.amxx"));
    if !ok {
        fails.push(format!("{os}: stats: no rank dump after the restart"));
        return fails;
    }
    let loaded = read_rank_dump(&std::fs::read_to_string(log).unwrap_or_default(), "loaded");
    println!("  stats: the restarted server loaded {} entries", loaded.len());
    let key = |v: &[RankEntry]| {
        let mut v = v.to_vec();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    };
    if key(&loaded) != key(&file) {
        fails.push(format!("{os}: stats: the loaded rank != tsstats.dat:\n    file {file:?}\n    rank {loaded:?}"));
    }
    if !sorted_by(&loaded, reverse_score) {
        fails.push(format!("{os}: stats: the loaded rank isn't ordered by the e2e score plugin's deaths - kills: {loaded:?}"));
    }
    for l in console_since(r, console_from).lines().filter(|l| l.contains("[TSX]") || l.contains("stats file")) {
        println!("  console: {l}");
        if l.contains("Couldn't load plugin") {
            fails.push(format!("{os}: stats: {l}"));
        }
    }
    fails
}

/// Comments plugins.ini's line for `name` out (`;name.amxx debug`) or back in.
fn set_plugin_enabled(ini: &Path, name: &str, on: bool) -> Result<(), String> {
    let line = format!("{name}.amxx debug");
    let text = std::fs::read_to_string(ini).map_err(|e| e.to_string())?;
    let mut found = false;
    let out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.trim_start_matches(';') == line {
                found = true;
                if on { line.clone() } else { format!(";{line}") }
            } else {
                l.to_owned()
            }
        })
        .collect();
    let mut out = out.join("\n") + "\n";
    if !found && on {
        out.push_str(&format!("; tsmod-amxx-rs e2e\n{line}\n"));
    }
    std::fs::write(ini, out).map_err(|e| e.to_string())
}

/// tsbot.inc's end-to-end test: `e2e/tsbot_e2e.sma` alone (the match's plugins commented out of
/// plugins.ini meanwhile) on a server without quota bots. It sets `ts_bot_backend tsbot_e2e`,
/// registers the backend, adds a bot through `addbot` and one directly, drives them, and logs
/// `CHECK` lines and `DONE` to `logs/tsbot_e2e.log`. Returns the failures.
fn tsbot_phase(r: &Root, testenv: &Path) -> Vec<String> {
    let os = r.os;
    let amxx = r.dir.join("ts/addons/amxmodx");
    let ini = amxx.join("configs/plugins.ini");
    let log = amxx.join(format!("logs/{BOT_PLUGIN}.log"));
    let mut fails = Vec::new();
    if running(r) {
        let _ = Command::new(testenv.join("stop.sh")).arg(os).status();
    }
    let dest = amxx.join(format!("plugins/{BOT_PLUGIN}.amxx"));
    let _ = std::fs::remove_file(&dest);
    let _ = std::fs::remove_file(&log);
    let setup = std::fs::copy(plugin_path(BOT_PLUGIN), &dest)
        .map_err(|e| e.to_string())
        .and_then(|_| PLUGINS.iter().try_for_each(|n| set_plugin_enabled(&ini, n, false)))
        .and_then(|()| set_plugin_enabled(&ini, BOT_PLUGIN, true));
    let restore = || {
        for n in PLUGINS {
            let _ = set_plugin_enabled(&ini, n, true);
        }
        let _ = set_plugin_enabled(&ini, BOT_PLUGIN, false);
    };
    if let Err(e) = setup {
        restore();
        return vec![format!("{os}: tsbot: {e}")];
    }
    if let Err(e) = run(Command::new(testenv.join("run.sh")).args([os, "--map", "ts_dojo", "--bots", "0"])) {
        restore();
        return vec![format!("{os}: tsbot: {e}")];
    }
    let done = wait_for(&log, Duration::from_secs(150), |t| t.contains("DONE"));
    quit(r, testenv);
    restore();
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    println!("--- {os} {BOT_PLUGIN} ({}) ---", log.display());
    for l in text.lines() {
        let msg = l.split_once(": ").map_or(l, |(_, m)| m);
        println!("  {msg}");
        if msg.starts_with("CHECK ") && msg.contains(" FAIL") {
            fails.push(format!("{os}: {msg}"));
        }
    }
    if !done {
        fails.push(format!("{os}: no DONE in {} (did tsx and the plugin load?)", log.display()));
    }
    fails
}

fn rcon(testenv: &Path, port: u16, cmd: &str) -> String {
    Command::new("python3")
        .arg("-I")
        .arg(testenv.join("rcon.py"))
        .args(["--port", &port.to_string(), cmd])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Runs one root; returns the failures.
fn e2e_root(r: &Root, testenv: &Path) -> Result<Vec<String>, String> {
    if running(r) {
        return Err(format!("a server is already running in {} (amxx-testenv/stop.sh {})", r.dir.display(), r.os));
    }
    install(r)?;
    if std::env::var_os("E2E_ONLY_TSBOT").is_some() {
        return Ok(tsbot_phase(r, testenv));
    }
    let logs = r.dir.join("ts/addons/amxmodx/logs");
    let started = std::time::SystemTime::now();
    let console_from = file_len(&qconsole(r));
    run(Command::new(testenv.join("run.sh")).args([r.os, "--map", "ts_dojo", "--bots", "4"]))?;
    let timeout = Duration::from_secs(std::env::var("E2E_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(150));
    let log_of = |name: &str| logs.join(format!("{name}.log"));
    let t0 = Instant::now();
    let mut modules = String::new();
    loop {
        let texts: Vec<String> = PLUGINS.iter().map(|n| std::fs::read_to_string(log_of(n)).unwrap_or_default()).collect();
        if modules.is_empty() && texts.iter().any(|t| t.contains("start")) {
            modules = rcon(testenv, r.port, "amxx modules");
        }
        if texts.iter().all(|t| t.contains("DONE")) || t0.elapsed() > timeout {
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    let mut fails = Vec::new();
    let done = PLUGINS.iter().all(|n| std::fs::read_to_string(log_of(n)).is_ok_and(|t| t.contains("DONE")));
    let stats_fails = if done && std::env::var_os("E2E_NO_STATS").is_none() {
        stats_round_trip(r, testenv, &log_of("tsx_e2e"), console_from)
    } else {
        let _ = Command::new(testenv.join("stop.sh")).arg(r.os).status();
        Vec::new()
    };

    // tsbot.inc's phase, on its own server, before the error logs are read.
    if std::env::var_os("E2E_NO_TSBOT").is_none() {
        fails.extend(tsbot_phase(r, testenv));
    }

    for name in PLUGINS {
        let log = log_of(name);
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        println!("--- {} {name} ({}) ---", r.os, log.display());
        for l in text.lines() {
            // log_to_file lines: `L <date> - <time>: <message>`
            let msg = l.split_once(": ").map_or(l, |(_, m)| m);
            println!("  {msg}");
            if msg.starts_with("CHECK ") && msg.contains(" FAIL") {
                fails.push(format!("{}: {msg}", r.os));
            }
        }
        if !text.contains("DONE") {
            fails.push(format!(
                "{}: no DONE in {} within {timeout:?} (did the modules and the plugin load? see {})",
                r.os,
                log.display(),
                logs.display()
            ));
        }
    }
    let wanted = |l: &str| MODULE_NAMES.iter().any(|m| l.contains(m));
    for l in modules.lines().filter(|l| wanted(l)) {
        println!("  amxx modules: {}", l.trim());
    }
    if !modules.is_empty() {
        for m in MODULE_NAMES {
            if !modules.lines().any(|l| l.contains(m) && l.to_lowercase().contains("running")) {
                fails.push(format!("{}: `amxx modules` doesn't list {m} as running", r.os));
            }
        }
    }
    // Native errors raised by the modules during this run (except the intended one).
    let ours = |l: &str| l.contains("[TSX]") || l.contains("[HAMSANDWICH]");
    if let Ok(dir) = std::fs::read_dir(&logs) {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let fresh = e.metadata().and_then(|m| m.modified()).is_ok_and(|m| m >= started);
            if !fresh {
                continue;
            }
            let t = std::fs::read_to_string(e.path()).unwrap_or_default();
            if name.starts_with("error_") {
                for l in t.lines().filter(|l| ours(l) && !EXPECTED_ERRORS.iter().any(|x| l.contains(x))) {
                    fails.push(format!("{}: {name}: {l}", r.os));
                }
            }
            if name.starts_with('L') && name.ends_with(".log") {
                for l in t.lines().filter(|l| ours(l)) {
                    println!("  amxx log: {}", l.split_once(": ").map_or(l, |(_, m)| m));
                }
            }
        }
    }
    fails.extend(stats_fails);
    Ok(fails)
}

fn e2e(names: &[String]) -> Result<(), String> {
    let testenv = env_path("AMXX_TESTENV", home().join("dev/hl/ref/amxx-testenv"));
    let scripting = env_path("AMXX_SCRIPTING", home().join("dev/hl/ref/extracted/amxmodx-1.10.0-git5487-linux/addons/amxmodx/scripting"));
    if !testenv.join("run.sh").exists() || !scripting.join("amxxpc").exists() {
        return Err(format!("amxx-testenv not found ({}, {}): see its README", testenv.display(), scripting.display()));
    }
    let roots = roots(names)?;
    build(true)?;
    for name in PLUGINS.into_iter().chain([SCORE_STOCK, SCORE_E2E, BOT_PLUGIN]) {
        compile_plugin(&scripting, name, &plugin_path(name))?;
    }
    let mut fails = Vec::new();
    for r in &roots {
        fails.extend(e2e_root(r, &testenv)?);
    }
    if fails.is_empty() {
        println!("e2e: all checks passed");
        Ok(())
    } else {
        Err(format!("e2e failures:\n  {}", fails.join("\n  ")))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let rest: Vec<String> = args.iter().skip(1).filter(|a| !a.starts_with("--")).cloned().collect();
    let r = match args.first().map(String::as_str) {
        Some("build") => build(flag("--release")),
        Some("dist") => dist(flag("--debug-info")),
        Some("lint") => lint(),
        Some("test") => test(),
        Some("e2e") => e2e(&rest),
        _ => Err("usage: cargo xtask build [--release] | dist [--debug-info] | lint | test | e2e [linux] [win] [linux64]".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}
