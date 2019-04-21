use failure::{ResultExt};
use log::debug;
use regex::Regex;
use std::fs::read_to_string;
// use std::io::Read;
use std::path::Path;
use log::{trace, error};
use lazy_static::lazy_static;
use std::collections::HashMap;

// use libc::{getuid, sysinfo};

const MODULE: &str = "Linux::util";
const WHEREIS_CMD: &str = "whereis";

pub const DF_CMD: &str = "df";
pub const LSBLK_CMD: &str = "lsblk";
pub const MOUNT_CMD: &str = "mount";
pub const FILE_CMD: &str = "file";
pub const UNAME_CMD: &str = "uname";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const GRUB_INSTALL_CMD: &str = "grub-install";


const REQUIRED_CMDS: &'static [&'static str] = &[
    DF_CMD, 
    LSBLK_CMD, 
    MOUNT_CMD,
    FILE_CMD,
    UNAME_CMD,
]; 

const OPTIONAL_CMDS: &'static [&'static str] = &[
    MOKUTIL_CMD,
    GRUB_INSTALL_CMD,
]; 


use crate::migrator::{
    linux::LinuxMigrator,
    common::{call, CmdRes},
    MigErrCtx, 
    MigError, 
    MigErrorKind,
    };


pub(crate) fn call_cmd(
    cmd: &str,
    args: &[&str],
    trim_stdout: bool,
) -> Result<CmdRes, MigError> {
    lazy_static! {
        static ref CMD_PATH: HashMap<String,Option<String>> = {
            let mut map = HashMap::new();
            for cmd in REQUIRED_CMDS {
                map.insert(
                    String::from(*cmd), 
                    Some(match whereis(cmd) {
                        Ok(cmd) => cmd,
                        Err(why) => {
                            let message = format!("cannot find required command {}", cmd);
                            error!("{}", message);
                            panic!("{}", message);
                        }
                    }));      
            }
            for cmd in OPTIONAL_CMDS {
                map.insert(
                    String::from(*cmd), 
                    match whereis(cmd) {
                        Ok(cmd) => Some(cmd),
                        Err(_why) => None, // TODO: check error codes
                    });      
            }
            map
        };
    }

    trace!("call_cmd: entered with cmd: '{}', args: {:?}, trim: {}", cmd, args, trim_stdout);
    if let Some(found_cmd) = CMD_PATH.get(cmd) {
        if let Some(valid_cmd) = found_cmd {
            Ok(call(valid_cmd, args, trim_stdout)?) 
        } else {
            Err(MigError::from_remark(MigErrorKind::NotFound,&format!("{}::call_cmd: {} is not available", MODULE, cmd)))    
        }
    } else {
        Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::call_cmd: {} is not in the list of checked commands", MODULE, cmd)))
    }
}


pub(crate) fn check_work_file(file: &str, work_dir: &str ) -> Result<Option<(String,String)>, MigError> {    
    trace!("{}::check_work_file: entered with file: '{}', work_dir: '{}'", MODULE, file, work_dir);
    let checked_path = 
        if file.starts_with("/") || file.starts_with("./") || file.starts_with("../") {
            if file_exists(file) {
                Some(String::from(file))
            } else {
                None
            }
        } else {            
            let search = if work_dir.ends_with("/") {
                format!("{}{}",work_dir, file)
            } else {
                format!("{}/{}",work_dir, file)
            };
            if file_exists(&search) {
                Some(search)
            } else {
                None
            }
        };
    
    debug!("{}::check_work_file: checked path for '{}': '{:?}'", MODULE, file, &checked_path);

    if let Some(path) = checked_path {
        let args: Vec<&str> =  vec!["-b", "-i", &path];
        let cmd_res = call_cmd("file", &args , true)?;       
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(MigErrorKind::InvParam , &format!("{}::new: failed determine type for file {}", MODULE, path)));
        }
        Ok(Some((path, cmd_res.stdout)))
    } else {
        Ok(None)
    }

}


pub fn parse_file(fname: &str, regex: &Regex) -> Result<Option<String>, MigError> {
    let os_info = read_to_string(fname).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("File read '{}'", fname),
    ))?;

    for line in os_info.lines() {
        debug!("{}::parse_file: line: '{}'", MODULE, line);

        if let Some(cap) = regex.captures(line) {
            return Ok(Some(String::from(cap.get(1).unwrap().as_str())));
        };
    }

    Ok(None)
}

pub fn dir_exists(name: &str) -> Result<bool,MigError> {
    let path = Path::new(name);
    if path.exists()  {
        Ok(path.metadata()
            .context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("{}::dir_exists: failed to retrieve metadata for path: {}", MODULE, name)))?
            .file_type().is_dir())
    } else {
        Ok(false)
    }
}



pub fn file_exists(file: &str) -> bool {    
    Path::new(file).exists()    
}

pub fn whereis(cmd: &str) -> Result<String, MigError> {
    let args: [&str; 2] = ["-b", cmd];
    let cmd_res = call(WHEREIS_CMD, &args, true).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("{}::whereis: failed for '{}'", MODULE, cmd),
    ))?;
    if cmd_res.status.success() {
        if cmd_res.stdout.is_empty() {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::whereis: no command output for {}", MODULE, cmd),
            ))
        } else {
            let mut words = cmd_res.stdout.split(" ");
            if let Some(s) = words.nth(1) {
                Ok(String::from(s))
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("{}::whereis: command not found: '{}'", MODULE, cmd),
                ))
                //
            }
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!(
                "{}::whereis: command failed for {}: {}",
                MODULE,
                cmd,
                cmd_res.status.code().unwrap_or(0)
            ),
        ))
    }
}

pub fn command_exists(cmd: &str) -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
}

pub fn exec_command(cmd: &str) -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
}