//! radiance-lemonade: start the radiance inference server from the flags a Lemonade backend
//! definition can pass.
//!
//! radiance (codeberg.org/StillDeadcode/radiance) takes its settings only from a YAML file
//! (`radiance --config FILE`), and a schema-v1 backend definition can pass argv and environment but
//! cannot write a file. This launcher takes flags, writes the file, and replaces itself with
//! radiance (exec), so the process Lemonade started and stops is the server itself.
//!
//!   radiance-lemonade --model PATH --port N --name ID [--host ADDR] [--ctx N] [--device LIST]
//!                     [--kernels LIST] [--spec-tokens N] [--max-seqs N]
//!                     [--radiance PATH] [--home DIR] [--config-dir DIR] [--print]
//!
//! --name is the model id requests carry: radiance answers 404 for any other id, so the definition
//! passes Lemonade's own model name. --device is a HIP device list (HIP_VISIBLE_DEVICES); empty
//! means every card. --ctx 0 or empty takes the model's training context. --print writes the YAML
//! to stdout and exits.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

#[derive(Debug, Default, PartialEq)]
struct Opts {
    model: String,
    port: u16,
    name: String,
    host: String,
    ctx: Option<u64>,
    device: Option<String>,
    kernels: Option<String>,
    spec_tokens: Option<u32>,
    max_seqs: Option<u32>,
    radiance: Option<PathBuf>,
    home: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    print: bool,
}

fn usage() -> &'static str {
    "usage: radiance-lemonade --model PATH --port N --name ID [--host ADDR] [--ctx N] [--device LIST] \
     [--kernels LIST] [--spec-tokens N] [--max-seqs N] [--radiance PATH] [--home DIR] [--config-dir DIR] [--print]"
}

/// Empty values count as absent: a definition passes an option even when the user left it blank.
fn nonempty(v: String) -> Option<String> {
    let t = v.trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts { host: "127.0.0.1".into(), ..Default::default() };
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        if flag == "--print" {
            o.print = true;
            i += 1;
            continue;
        }
        if flag == "-h" || flag == "--help" {
            return Err(usage().into());
        }
        let val = args.get(i + 1).cloned().ok_or_else(|| format!("{flag} needs a value"))?;
        let num = |what: &str, v: &str| -> Result<u64, String> {
            v.trim().parse::<u64>().map_err(|_| format!("{what}: '{v}' is not a whole number"))
        };
        match flag {
            "--model" => o.model = val,
            "--port" => {
                o.port = u16::try_from(num("--port", &val)?).map_err(|_| "--port: out of range".to_string())?
            }
            "--name" => o.name = val,
            "--host" => o.host = val,
            "--ctx" => o.ctx = nonempty(val).map(|v| num("--ctx", &v)).transpose()?.filter(|&n| n > 0),
            "--device" => o.device = nonempty(val),
            "--kernels" => o.kernels = nonempty(val),
            "--spec-tokens" => o.spec_tokens = nonempty(val).map(|v| num("--spec-tokens", &v).map(|n| n as u32)).transpose()?,
            "--max-seqs" => o.max_seqs = nonempty(val).map(|v| num("--max-seqs", &v).map(|n| n as u32)).transpose()?,
            "--radiance" => o.radiance = nonempty(val).map(PathBuf::from),
            "--home" => o.home = nonempty(val).map(PathBuf::from),
            "--config-dir" => o.config_dir = nonempty(val).map(PathBuf::from),
            _ => return Err(format!("unknown flag {flag}\n{}", usage())),
        }
        i += 2;
    }
    if o.model.trim().is_empty() {
        return Err("--model is required".into());
    }
    if o.port == 0 {
        return Err("--port is required (1-65535)".into());
    }
    if o.name.trim().is_empty() {
        return Err("--name is required (the model id requests carry)".into());
    }
    Ok(o)
}

/// A YAML double-quoted scalar: backslash, quote and control characters escaped.
fn q(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn render(o: &Opts) -> String {
    let mut y = String::new();
    y.push_str("# written by radiance-lemonade; regenerated at every start\n");
    y.push_str("server:\n");
    y.push_str(&format!("  host: {}\n", q(&o.host)));
    y.push_str(&format!("  port: {}\n", o.port));
    if let Some(k) = &o.kernels {
        y.push_str(&format!("  kernels: {}\n", q(k)));
    }
    y.push_str("models:\n");
    y.push_str(&format!("  {}:\n", q(&o.name)));
    y.push_str(&format!("    model: {}\n", q(&o.model)));
    if let Some(c) = o.ctx {
        y.push_str(&format!("    max-model-len: {c}\n"));
    }
    if let Some(n) = o.max_seqs {
        y.push_str(&format!("    max-num-seqs: {n}\n"));
    }
    if let Some(n) = o.spec_tokens {
        y.push_str(&format!("    num-speculative-tokens: {n}\n"));
    }
    y
}

fn exe_dir() -> Option<PathBuf> {
    env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
}

fn find_radiance(o: &Opts) -> Result<PathBuf, String> {
    if let Some(p) = &o.radiance {
        return Ok(p.clone());
    }
    if let Some(p) = env::var_os("RADIANCE_BIN").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    if let Some(d) = exe_dir() {
        let p = d.join("radiance");
        if p.is_file() {
            return Ok(p);
        }
    }
    Err("radiance not found: pass --radiance, set RADIANCE_BIN, or install this launcher beside it".into())
}

/// RADIANCE_HOME (the plugin folder): flag, then environment, then the build tree or install
/// layout beside the binary. None leaves radiance on its compiled-in default.
fn find_home(o: &Opts, radiance: &Path) -> Option<OsString> {
    if let Some(h) = &o.home {
        return Some(h.clone().into_os_string());
    }
    if let Some(h) = env::var_os("RADIANCE_HOME").filter(|v| !v.is_empty()) {
        return Some(h);
    }
    let dir = radiance.parent()?;
    for cand in [dir.join("../radiance_home"), dir.join("../share/radiance")] {
        if cand.join("kernels").is_dir() {
            return Some(cand.into_os_string());
        }
    }
    None
}

fn write_config(o: &Opts, yaml: &str) -> Result<PathBuf, String> {
    let dir = o
        .config_dir
        .clone()
        .or_else(|| env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()).map(PathBuf::from))
        .unwrap_or_else(env::temp_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("radiance-lemonade-{}.yaml", o.port));
    let tmp = dir.join(format!(".radiance-lemonade-{}.yaml.{}", o.port, std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.write_all(yaml.as_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    drop(f);
    fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let o = match parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("radiance-lemonade: {e}");
            exit(2);
        }
    };
    let yaml = render(&o);
    if o.print {
        print!("{yaml}");
        return;
    }
    let radiance = match find_radiance(&o) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("radiance-lemonade: {e}");
            exit(2);
        }
    };
    let config = match write_config(&o, &yaml) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("radiance-lemonade: cannot write the config: {e}");
            exit(1);
        }
    };
    let mut cmd = Command::new(&radiance);
    cmd.arg("--config").arg(&config);
    if let Some(h) = find_home(&o, &radiance) {
        cmd.env("RADIANCE_HOME", h);
    }
    if let Some(d) = &o.device {
        cmd.env("HIP_VISIBLE_DEVICES", d);
    }
    eprintln!("radiance-lemonade: {} --config {}", radiance.display(), config.display());
    let err = cmd.exec(); // returns only on failure
    eprintln!("radiance-lemonade: exec {}: {err}", radiance.display());
    exit(127);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn minimal_flags_render_one_model() {
        let o = parse(&a(&["--model", "/m/x.rad", "--port", "13470", "--name", "user.x"])).unwrap();
        assert_eq!(
            render(&o),
            "# written by radiance-lemonade; regenerated at every start\nserver:\n  host: \"127.0.0.1\"\n  port: 13470\nmodels:\n  \"user.x\":\n    model: \"/m/x.rad\"\n"
        );
    }

    #[test]
    fn blank_options_are_absent() {
        let o = parse(&a(&[
            "--model", "m", "--port", "1", "--name", "n", "--ctx", "", "--device", " ", "--kernels", "", "--spec-tokens", "",
        ]))
        .unwrap();
        assert_eq!((o.ctx, o.device, o.kernels, o.spec_tokens), (None, None, None, None));
    }

    #[test]
    fn ctx_zero_means_training_context() {
        let o = parse(&a(&["--model", "m", "--port", "1", "--name", "n", "--ctx", "0"])).unwrap();
        assert_eq!(o.ctx, None);
        assert!(!render(&o).contains("max-model-len"));
    }

    #[test]
    fn options_render_under_the_model() {
        let o = parse(&a(&[
            "--model", "m", "--port", "9", "--name", "n", "--ctx", "4096", "--spec-tokens", "0", "--max-seqs", "2",
            "--kernels", "libr3d,libavx,libref",
        ]))
        .unwrap();
        let y = render(&o);
        assert!(y.contains("  kernels: \"libr3d,libavx,libref\"\n"));
        assert!(y.contains("    max-model-len: 4096\n    max-num-seqs: 2\n    num-speculative-tokens: 0\n"));
    }

    #[test]
    fn quoting_escapes_yaml_specials() {
        assert_eq!(q("a\"b\\c: d\n"), "\"a\\\"b\\\\c: d\\n\"");
        assert_eq!(q("\u{1}"), "\"\\x01\"");
    }

    #[test]
    fn required_and_bad_values_are_refused() {
        assert!(parse(&a(&["--port", "1", "--name", "n"])).is_err());
        assert!(parse(&a(&["--model", "m", "--name", "n"])).is_err());
        assert!(parse(&a(&["--model", "m", "--port", "1"])).is_err());
        assert!(parse(&a(&["--model", "m", "--port", "70000", "--name", "n"])).is_err());
        assert!(parse(&a(&["--model", "m", "--port", "1", "--name", "n", "--ctx", "big"])).is_err());
        assert!(parse(&a(&["--model", "m", "--port", "1", "--name", "n", "--bogus", "x"])).is_err());
        assert!(parse(&a(&["--model", "m", "--port"])).is_err());
    }

    #[test]
    fn config_file_is_private_and_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let dir = env::temp_dir().join(format!("rl-test-{}", std::process::id()));
        let o = Opts { port: 4242, config_dir: Some(dir.clone()), ..Default::default() };
        let p = write_config(&o, "one\n").unwrap();
        let p2 = write_config(&o, "two\n").unwrap();
        assert_eq!(p, p2);
        assert_eq!(fs::read_to_string(&p).unwrap(), "two\n");
        assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        fs::remove_dir_all(dir).unwrap();
    }
}
