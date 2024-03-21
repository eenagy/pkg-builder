use crate::v1_build::packager::{BuildConfig, BackendBuildEnv};
use std::process::Command;

pub struct SbuildZig {
    config: BuildConfig,
}

// TODO: this is not the finished implementation, use default chroot for now 
impl SbuildZig {
    pub fn new(config: BuildConfig) -> Self {
        return SbuildZig { config: config };
    }

    fn get_build_name(&self) -> String {
        return format!("{}-{}-zig", self.config.codename(), self.config.arch());
    }
}

impl BackendBuildEnv for SbuildZig {
  
    fn clean(&self) -> Result<(), String> {
        let chroot_prefix = self.get_build_name();

        // Clean up previous chroots
        let cleanup_result = Command::new("sudo")
            .arg("rm")
            .args(&["-rf", &format!("/etc/sbuild/chroot/{}", chroot_prefix)])
            .args(&["-rf", &format!("/etc/schroot/chroot.d/{}*", chroot_prefix)])
            .args(&["-rf", &format!("/srv/chroot/{}", chroot_prefix)])
            .status();

        if let Err(err) = cleanup_result {
            return Err(format!("Failed to clean up previous chroots: {}", err));
        }
        Ok(())
    }

    fn create(&self) -> Result<(), String> {
        let chroot_prefix = self.get_build_name();

        // Create new chroot
        let create_result = Command::new("sudo")
            .arg("sbuild-createchroot")
            .arg("--merged-usr")
            .arg("--chroot-prefix")
            .arg(&chroot_prefix)
            .arg(&self.config.codename())
            .arg(&format!("/srv/chroot/{}", chroot_prefix))
            .arg("http://deb.debian.org/debian")
            .status();

        if let Err(err) = create_result {
            return Err(format!("Failed to create new chroot: {}", err));
        }

        Ok(())
    }
    fn build(&self) -> Result<(), String> {
        // Create new chroot
        let create_result = Command::new("sbuild")
            .arg("-c")
            .arg(self.get_build_name())
            .arg(self.config.codename())
            .status();

        if let Err(err) = create_result {
            return Err(format!("Failed to build package: {}", err));
        }

        Ok(())
    }
}
