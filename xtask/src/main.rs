//! Build, package and test tooling: `cargo xtask <command>`.
//!
//! ```text
//! build [--release]          tsx and tsfun for i686 Linux and Windows
//! dist [--debug-info]        release build -> dist/<platform>/addons/amxmodx/modules/ (stripped
//!                            unless --debug-info), with AMX Mod X's file names
//! lint                       cargo fmt --check, clippy -D warnings on both targets
//! test                       unit tests on the host and on i686 Linux (the ABI layout tests)
//! e2e [linux] [win]          modules + e2e/tsx_e2e.sma on the amxx-testenv roots, a bot match on
//!                            ts_dojo, then the plugin's checks (default: linux)
//! ```
//!
//! Environment: `FWGS_DIR` (FWGS headers, default `../xash3d-fwgs` via .cargo/config.toml),
//! `AMXX_TESTENV` (default `~/dev/hl/ref/amxx-testenv`), `AMXX_SCRIPTING` (amxxpc's directory),
//! `AMXX_LINUX_ROOT` / `AMXX_WIN_ROOT` (default `~/games/amxx_rehlds_{linux,win}`),
//! `E2E_TIMEOUT` (seconds to wait for the plugin, default 150).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

/// A build target and the file names AMX Mod X loads.
struct Target {
    triple: &'static str,
    platform: &'static str,
    /// (cargo output file, AMX Mod X module file).
    files: [(&'static str, &'static str); 2],
    strip: &'static str,
}

const TARGETS: [Target; 2] = [
    Target {
        triple: "i686-unknown-linux-gnu",
        platform: "linux-i686",
        files: [("libtsx.so", "tsx_amxx_i386.so"), ("libtsfun.so", "tsfun_amxx_i386.so")],
        strip: "strip",
    },
    Target {
        triple: "i686-pc-windows-gnu",
        platform: "windows-i686",
        files: [("tsx.dll", "tsx_amxx.dll"), ("tsfun.dll", "tsfun_amxx.dll")],
        strip: "i686-w64-mingw32-strip",
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

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn build(release: bool) -> Result<(), String> {
    for t in &TARGETS {
        let mut c = cargo();
        c.args(["build", "-p", "tsx", "-p", "tsfun", "--target", t.triple]);
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
fn check_windows_dll(dll: &Path, metamod: bool) -> Result<(), String> {
    let out = Command::new("i686-w64-mingw32-objdump").arg("-p").arg(dll).output().map_err(|e| format!("objdump: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    for bad in ["libgcc", "libwinpthread", "libstdc"] {
        if text.lines().any(|l| l.contains("dll name:") && l.contains(bad)) {
            return Err(format!("{} imports a MinGW runtime DLL ({bad})", dll.display()));
        }
    }
    // tsx is a metamod plugin too: metamod looks up the undecorated stdcall name.
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
                check_windows_dll(&dest, to.starts_with("tsx"))?;
            }
            eprintln!("  {}", dest.display());
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
    run(cargo().args(["test", "--workspace"]))?;
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
            other => Err(format!("unknown e2e root {other} (linux|win)")),
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

fn compile_plugin(scripting: &Path, out: &Path) -> Result<(), String> {
    let sma = root().join("e2e/tsx_e2e.sma");
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut c = Command::new(scripting.join("amxxpc"));
    c.env("LD_LIBRARY_PATH", scripting)
        .arg(&sma)
        .arg(format!("-i{}", scripting.join("include").display()))
        .arg(format!("-o{}", out.display()));
    // amxxpc writes `<out>.amx` scratch files next to the current directory.
    c.current_dir(out.parent().unwrap());
    let o = c.output().map_err(|e| format!("amxxpc: {e}"))?;
    if !o.status.success() || !out.exists() {
        return Err(format!("amxxpc failed:\n{}", String::from_utf8_lossy(&o.stdout)));
    }
    Ok(())
}

fn install(r: &Root, plugin: &Path) -> Result<(), String> {
    let amxx = r.dir.join("ts/addons/amxmodx");
    if !amxx.join("configs/modules.ini").exists() {
        return Err(format!("{} has no AMX Mod X: run amxx-testenv/setup.sh {}", r.dir.display(), r.os));
    }
    for (from, to) in r.target.files {
        let dest = amxx.join("modules").join(to);
        let _ = std::fs::remove_file(&dest); // never write through a hardlink
        std::fs::copy(built(r.target, true, from), &dest).map_err(|e| format!("{from}: {e}"))?;
    }
    ensure_line(&amxx.join("configs/modules.ini"), "tsx")?;
    ensure_line(&amxx.join("configs/modules.ini"), "tsfun")?;
    let dest = amxx.join("plugins/tsx_e2e.amxx");
    let _ = std::fs::remove_file(&dest);
    std::fs::copy(plugin, &dest).map_err(|e| e.to_string())?;
    ensure_line(&amxx.join("configs/plugins.ini"), "tsx_e2e.amxx debug")?;
    let _ = std::fs::remove_file(amxx.join("logs/tsx_e2e.log"));
    Ok(())
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
fn e2e_root(r: &Root, testenv: &Path, plugin: &Path) -> Result<Vec<String>, String> {
    if running(r) {
        return Err(format!("a server is already running in {} (amxx-testenv/stop.sh {})", r.dir.display(), r.os));
    }
    install(r, plugin)?;
    let logs = r.dir.join("ts/addons/amxmodx/logs");
    let started = std::time::SystemTime::now();
    run(Command::new(testenv.join("run.sh")).args([r.os, "--map", "ts_dojo", "--bots", "4"]))?;
    let timeout = Duration::from_secs(std::env::var("E2E_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(150));
    let log = logs.join("tsx_e2e.log");
    let t0 = Instant::now();
    let mut modules = String::new();
    let done = loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if modules.is_empty() && text.contains("start:") {
            modules = rcon(testenv, r.port, "amxx modules");
        }
        if text.contains("DONE") {
            break true;
        }
        if t0.elapsed() > timeout {
            break false;
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    let _ = Command::new(testenv.join("stop.sh")).arg(r.os).status();

    let mut fails = Vec::new();
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    println!("--- {} ({}) ---", r.os, log.display());
    for l in text.lines() {
        // log_to_file lines: `L <date> - <time>: <message>`
        let msg = l.split_once(": ").map_or(l, |(_, m)| m);
        println!("  {msg}");
        if msg.starts_with("CHECK ") && msg.contains(" FAIL") {
            fails.push(format!("{}: {msg}", r.os));
        }
    }
    if !done {
        fails.push(format!(
            "{}: no DONE in {} within {timeout:?} (did tsx and the plugin load? see {})",
            r.os,
            log.display(),
            logs.display()
        ));
    }
    for l in modules.lines().filter(|l| l.to_lowercase().contains("ts")) {
        println!("  amxx modules: {}", l.trim());
    }
    if !modules.is_empty() && !modules.lines().any(|l| l.contains("TSX") && l.to_lowercase().contains("running")) {
        fails.push(format!("{}: `amxx modules` doesn't list TSX as running", r.os));
    }
    // Native errors raised by tsx during this run.
    if let Ok(dir) = std::fs::read_dir(&logs) {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let fresh = e.metadata().and_then(|m| m.modified()).is_ok_and(|m| m >= started);
            if name.starts_with("error_") && fresh {
                let t = std::fs::read_to_string(e.path()).unwrap_or_default();
                for l in t.lines().filter(|l| l.contains("[TSX]")) {
                    fails.push(format!("{}: {name}: {l}", r.os));
                }
            }
            if name.starts_with('L') && name.ends_with(".log") && fresh {
                let t = std::fs::read_to_string(e.path()).unwrap_or_default();
                for l in t.lines().filter(|l| l.contains("[TSX]")) {
                    println!("  amxx log: {}", l.split_once(": ").map_or(l, |(_, m)| m));
                }
            }
        }
    }
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
    let plugin = root().join("target/e2e/tsx_e2e.amxx");
    compile_plugin(&scripting, &plugin)?;
    let mut fails = Vec::new();
    for r in &roots {
        fails.extend(e2e_root(r, &testenv, &plugin)?);
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
        _ => Err("usage: cargo xtask build [--release] | dist [--debug-info] | lint | test | e2e [linux] [win]".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}
