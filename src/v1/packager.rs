use eyre::{eyre, Result};


use crate::v1::pkg_config::PkgConfig;

use super::distribution::{
    debian::bookworm::BookwormPackager,
};

pub trait Packager {
    type BuildEnv: BackendBuildEnv;
    fn new(config: PkgConfig, config_root: String) -> Self;
    fn package(&self) -> Result<()>;

    fn get_build_env(&self) -> Result<Self::BuildEnv>;
}
pub struct DistributionPackager {
    config: PkgConfig,
    config_root: String,
}

pub trait BackendBuildEnv {
    fn clean(&self) -> Result<()>;
    fn create(&self) -> Result<()>;
    fn build(&self) -> Result<()>;
}

impl DistributionPackager {
    pub fn new(config: PkgConfig, config_root: String) -> Self {
        DistributionPackager {
            config,
            config_root,
        }
    }
    pub fn package(&self) -> Result<()> {
        let config = self.config.clone();

        match self.config.build_env.codename.clone().as_str() {
            "bookworm" | "debian 12" => {

                let packager = BookwormPackager::new(config, self.config_root.clone());
                packager.package()?;
            }
            "jammy jellyfish" | "ubuntu 22.04" => todo!(),
            invalid_codename => {
                return Err(eyre!(format!(
                    "Invalid codename '{}' specified",
                    invalid_codename
                )));
            }
        }
        Ok(())
    }
    pub fn clean_build_env(&self) -> Result<()> {
        let config = self.config.clone();

        match self.config.build_env.codename.clone().as_str() {
            "bookworm" | "debian 12" => {
                let packager = BookwormPackager::new(config, self.config_root.clone());

                let build_env = packager.get_build_env()?;
                build_env.clean()?;
            }
            "jammy jellyfish" | "ubuntu 22.04" => todo!(),
            invalid_codename => {
                return Err(eyre!(format!(
                    "Invalid codename '{}' specified",
                    invalid_codename
                )));
            }
        }
        Ok(())
    }
    pub fn create_build_env(&self) -> Result<()> {
        let config = self.config.clone();

        match self.config.build_env.codename.clone().as_str() {
            "bookworm" | "debian 12" => {
                let packager = BookwormPackager::new(config, self.config_root.clone());
                let build_env = packager.get_build_env()?;
                build_env.create()?;
            }
            "jammy jellyfish" | "ubuntu 22.04" => todo!(),
            invalid_codename => {
                return Err(eyre!(format!(
                    "Invalid codename '{}' specified",
                    invalid_codename
                )));
            }
        }
        Ok(())
    }
}