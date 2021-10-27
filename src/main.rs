use std::{
    borrow::Cow,
    ffi::OsStr,
    fmt::Display,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use rusqlite::{params_from_iter, Connection};

static COOKIE_FILE_HEADER: &str = "# Netscape HTTP Cookie File
# http://curl.haxx.se/rfc/cookie_spec.html
# This is a generated file!  Do not edit.
# ALL SPACES MUST BE TABS! - IT WILL THROW AN ERROR!";

#[derive(Clone, Debug, Parser)]
struct Opts {
    /// grab cookies for these hosts
    hosts: Vec<String>,

    /// save output to file
    #[clap(short, long)]
    output: Option<String>,
}

#[derive(Clone, Debug)]
struct MozCookie {
    host: String,
    path: String,
    expiry: i64,
    name: String,
    value: String,
}

impl MozCookie {
    fn fmt(&self) -> MozCookieFmt {
        MozCookieFmt(self)
    }
}

struct MozCookieFmt<'a>(&'a MozCookie);

impl Display for MozCookieFmt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\tTRUE\t{}\tFALSE\t{}\t{}\t{}",
            self.0.host, self.0.path, self.0.expiry, self.0.name, self.0.value
        )
    }
}

fn main() {
    let opts = Opts::parse();
    if let Err(e) = run(&opts) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn run(opts: &Opts) -> anyhow::Result<()> {
    if opts.hosts.is_empty() {
        return Ok(());
    }

    let db_path = get_db_path().ok_or_else(|| io::Error::new(
        io::ErrorKind::NotFound,
        "cookie db not found",
    ))?;

    let connection = Connection::open(&db_path)?;

    let hosts_formatter = build_formatter(opts.hosts.len());
    let query = format!(
        "select name, value, host, path, expiry \
        from moz_cookies \
        where host in ({})",
        hosts_formatter
    );

    let mut s = connection.prepare(&query)?;
    let cookies: Result<Vec<_>, _> = s
        .query_map(params_from_iter(&opts.hosts), |row| {
            Ok(MozCookie {
                host: row.get("host")?,
                path: row.get("path")?,
                expiry: row.get("expiry")?,
                name: row.get("name")?,
                value: row.get("value")?,
            })
        })?
        .collect();

    if let Some(path) = &opts.output {
        save_to_path(path, &cookies?)?;
    } else {
        format_stdout(&cookies?)?;
    }

    Ok(())
}

fn save_to_path(path: &str, cookies: &[MozCookie]) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "{}\n", COOKIE_FILE_HEADER)?;

    for cookie in cookies {
        writeln!(file, "{}", cookie.fmt())?;
    }

    Ok(())
}

fn format_stdout(cookies: &[MozCookie]) -> io::Result<()> {
    let handle = io::stdout();
    let mut lock = handle.lock();

    writeln!(lock, "{}\n", COOKIE_FILE_HEADER)?;

    for cookie in cookies {
        writeln!(lock, "{}", cookie.fmt())?;
    }

    Ok(())
}

fn build_formatter(len: usize) -> Cow<'static, str> {
    match len {
        0 => Cow::from(""),
        1 => Cow::from("?"),
        n => {
            let len = n - 1;
            let mut buf = String::from("?");
            for _ in 0..len {
                buf.push_str(",?");
            }
            Cow::from(buf)
        }
    }
}

fn get_db_path() -> Option<PathBuf> {
    let target = OsStr::new("cookies.sqlite");
    let path = dirs::data_dir()?.join("Mozilla\\Firefox\\Profiles");
    match search(&path, target) {
        // On Linux, Firefox eschews standard config locations.
        None => {
            let path = dirs::home_dir()?.join(".mozilla/firefox");
            search(&path, target)
        }
        path => path,
    }
}

fn search(path: impl AsRef<Path>, target: &OsStr) -> Option<PathBuf> {
    let path = path.as_ref();
    let mut walker = walkdir::WalkDir::new(path)
        .contents_first(true)
        .into_iter()
        .filter_map(|entry| {
            let path = entry.ok()?.into_path();
            if path.file_name() == Some(target) {
                Some(path)
            } else {
                None
            }
        });
    walker.next()
}
