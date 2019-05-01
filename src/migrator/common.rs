//pub mod mig_error;
use failure::ResultExt;
use log::trace;
use std::fmt::{self, Display, Formatter};
use std::process::{Command, ExitStatus, Stdio};
use std::fs::read_to_string;
use std::path::Path;
use log::{debug};
use regex::Regex;

pub mod stage_info;
pub use stage_info::{Stage1Info, Stage2Info};

pub mod mig_error;

pub mod os_release;
pub use os_release::OSRelease;

pub mod balena_cfg_json;
pub mod config;
pub mod config_helper;
pub mod file_info;
pub mod logger;
pub use logger::Logger;


pub use self::mig_error::{MigErrCtx, MigError, MigErrorKind};
pub use self::config::{Config, MigMode};
pub use self::file_info::{FileInfo, FileType};

const MODULE: &str = "migrator::common";
pub const STAGE2_CFG_FILE: &str = "/etc/balena-stage2.yml";

#[derive(Debug)]
pub enum OSArch {
    AMD64,
    ARMHF,
    I386,
    /*
        ARM64,
        ARMEL,
        MIPS,
        MIPSEL,
        Powerpc,
        PPC64EL,
        S390EX,
    */
}

impl Display for OSArch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub(crate) struct CmdRes {
    pub stdout: String,
    pub stderr: String,
    pub status: ExitStatus,
}

pub fn parse_file(fname: &str, regex: &Regex) -> Result<Option<Vec<String>>, MigError> {
    let os_info = read_to_string(fname).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("File read '{}'", fname),
    ))?;

    for line in os_info.lines() {
        debug!("parse_file: line: '{}'", line);

        if let Some(ref captures) = regex.captures(line) {
            let mut results: Vec<String> = Vec::new();
            for cap in captures.iter() {
                if let Some(cap) = cap {
                    results.push(String::from(cap.as_str()));
                } else {
                    results.push(String::from(""));
                }
            }
            return Ok(Some(results));
        };
    }

    Ok(None)
}

pub fn dir_exists(name: &str) -> Result<bool, MigError> {
    let path = Path::new(name);
    if path.exists() {
        Ok(path
            .metadata()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "{}::dir_exists: failed to retrieve metadata for path: {}",
                    MODULE, name
                ),
            ))?
            .file_type()
            .is_dir())
    } else {
        Ok(false)
    }
}

pub fn file_exists(file: &str) -> bool {
    Path::new(file).exists()
}

pub(crate) fn call(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    trace!("call(): '{}' called with {:?}, {}", cmd, args, trim_stdout);

    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::call: failed to execute: command {} '{:?}'",
                MODULE, cmd, args
            ),
        ))?;

    Ok(CmdRes {
        stdout: match trim_stdout {
            true => String::from(String::from_utf8_lossy(&output.stdout).trim()),
            false => String::from(String::from_utf8_lossy(&output.stdout)),
        },
        stderr: String::from(String::from_utf8_lossy(&output.stderr)),
        status: output.status,
    })
}

pub fn check_tcp_connect(host: &str, port: u16, timeout: u64) -> Result<(), MigError> {
    use std::net::{Shutdown, TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let url = format!("{}:{}", host, port);
    let mut addrs_iter = url.to_socket_addrs().context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "{}::check_tcp_connect: failed to resolve host address: '{}'",
            MODULE, url
        ),
    ))?;

    if let Some(ref sock_addr) = addrs_iter.next() {
        let tcp_stream = TcpStream::connect_timeout(sock_addr, Duration::new(timeout, 0)).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "{}::check_tcp_connect: failed to connect to: '{}' with timeout: {}",
                    MODULE, url, timeout
                ),
            ),
        )?;

        let _res = tcp_stream.shutdown(Shutdown::Both);
        Ok(())
    } else {
        Err(MigError::from_remark(
            MigErrorKind::InvState,
            &format!(
                "{}::check_tcp_connect: no results from name resolution for: '{}",
                MODULE, url
            ),
        ))
    }
}

const GIB_SIZE: u64 = 1024 * 1024 * 1024;
const MIB_SIZE: u64 = 1024 * 1024;
const KIB_SIZE: u64 = 1024;

pub fn format_size_with_unit(size: u64) -> String {
    if size > (10 * GIB_SIZE) {
        format!("{} GiB", size / GIB_SIZE)
    } else if size > (10 * MIB_SIZE) {
        format!("{} MiB", size / MIB_SIZE)
    } else if size > (10 * KIB_SIZE) {
        format!("{} KiB", size / KIB_SIZE)
    } else {
        format!("{} B", size)
    }
}
