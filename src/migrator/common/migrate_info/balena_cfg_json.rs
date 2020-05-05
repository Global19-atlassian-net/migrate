use crate::{
    common::{
        check_tcp_connect, file_info::RelFileInfo, Config, FileInfo, MigErrCtx, MigError,
        MigErrorKind,
    },
    defs::BALENA_API_PORT,
};

use crate::linux::linux_common::mktemp;
use failure::ResultExt;
use log::{error, info};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct BalenaCfgJson {
    config: HashMap<String, Value>,
    file: RelFileInfo,
    modified: bool,
}

impl BalenaCfgJson {
    pub fn new(cfg_file: FileInfo) -> Result<BalenaCfgJson, MigError> {
        Ok(BalenaCfgJson {
            config: serde_json::from_reader(BufReader::new(File::open(&cfg_file.path).context(
                MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("new: cannot open file '{}'", cfg_file.path.display()),
                ),
            )?))
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to parse json from file '{}'",
                    cfg_file.path.display()
                ),
            ))?,
            file: cfg_file.to_rel_fileinfo()?,
            modified: false,
        })
    }

    pub fn write(&mut self, work_dir: &Path) -> Result<PathBuf, MigError> {
        let new_path = mktemp(false, Some("config.json"), Some(work_dir.to_path_buf())).context(
            MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to create temporary file"),
        )?;

        let out_file =
            OpenOptions::new()
                .write(true)
                .open(&new_path)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to open file for writing: '{}'", new_path.display()),
                ))?;

        serde_json::to_writer(out_file, &self.config).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed save modified config.json to '{}'",
                new_path.display()
            ),
        ))?;

        self.modified = false;

        Ok(new_path)
    }

    pub fn check(&self, config: &Config) -> Result<(), MigError> {
        // TODO: app_name is not checked
        info!("Configured for application id: {}", self.get_app_id()?);

        if config.is_check_api() {
            let api_endpoint = &self.get_api_endpoint()?;

            let api_url = Url::parse(&api_endpoint).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to parse balena api url '{}'", api_endpoint),
            ))?;

            if let Some(api_host) = api_url.host() {
                let api_host = api_host.to_string();
                let api_port = if let Some(api_port) = api_url.port() {
                    api_port
                } else {
                    BALENA_API_PORT
                };

                if let Ok(_v) = check_tcp_connect(&api_host, api_port, config.get_check_timeout()) {
                    info!("connection to api: {}:{} is ok", api_host, api_port);
                } else {
                    error!(
                        "failed to connect to api server @ {}:{} your device might not come online",
                        api_endpoint, api_port
                    );
                    return Err(MigError::displayed());
                }
            } else {
                error!(
                    "failed to parse api server url from config.json: {}",
                    api_endpoint
                );
                return Err(MigError::displayed());
            }
        }

        if config.is_check_vpn() {
            let vpn_endpoint = self.get_vpn_endpoint()?;
            let vpn_port = self.get_vpn_port()? as u16;
            if let Ok(_v) = check_tcp_connect(&vpn_endpoint, vpn_port, config.get_check_timeout()) {
                // TODO: call a command on API instead of just connecting
                info!("connection to vpn: {}:{} is ok", vpn_endpoint, vpn_port);
            } else {
                error!(
                    "failed to connect to vpn server @ {}:{} your device might not come online",
                    vpn_endpoint, vpn_port
                );
                return Err(MigError::displayed());
            }
        }

        Ok(())
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn get_size(&self) -> u64 {
        self.file.size
    }

    pub fn get_rel_path(&self) -> &PathBuf {
        &self.file.rel_path
    }

    fn get_str_val(&self, name: &str) -> Result<String, MigError> {
        if let Some(value) = self.config.get(name) {
            if let Some(value) = value.as_str() {
                Ok(value.to_string())
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Invalid type encountered for '{}', expected String, found {:?} in config.json",
                        name, value
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Key could not be found in config.json: '{}'", name),
            ))
        }
    }

    fn get_uint_val(&self, name: &str) -> Result<u64, MigError> {
        if let Some(value) = self.config.get(name) {
            if let Some(value) = value.as_u64() {
                Ok(value)
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Invalid type encountered for '{}', expected uint, found {:?}",
                        name, value
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Key could not be found in config.json: '{}'", name),
            ))
        }
    }

    pub fn get_hostname(&self) -> Result<String, MigError> {
        self.get_str_val("hostname")
    }

    pub fn set_host_name(&mut self, hostname: &str) -> Option<String> {
        self.modified = true;

        if let Some(value) = self
            .config
            .insert("hostname".to_string(), Value::String(hostname.to_string()))
        {
            Some(value.to_string())
        } else {
            None
        }
    }

    pub fn get_app_id(&self) -> Result<u64, MigError> {
        self.get_uint_val("applicationId")
    }

    pub fn get_api_key(&self) -> Result<String, MigError> {
        self.get_str_val("apiKey")
    }

    pub fn get_api_endpoint(&self) -> Result<String, MigError> {
        self.get_str_val("apiEndpoint")
    }

    fn get_vpn_endpoint(&self) -> Result<String, MigError> {
        self.get_str_val("vpnEndpoint")
    }

    fn get_vpn_port(&self) -> Result<u64, MigError> {
        self.get_uint_val("vpnPort")
    }

    pub fn get_device_type(&self) -> Result<String, MigError> {
        self.get_str_val("deviceType")
    }
}
