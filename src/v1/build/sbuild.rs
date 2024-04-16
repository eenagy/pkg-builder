use crate::v1::packager::BackendBuildEnv;
use crate::v1::pkg_config::{LanguageEnv, PackageType, PkgConfig};
use eyre::{eyre, Result};
use log::info;
use rand::random;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, fs, io};

pub struct Sbuild {
    config: PkgConfig,
    build_files_dir: String,
    cache_dir: String,
}

impl Sbuild {
    pub fn new(config: PkgConfig, build_files_dir: String) -> Sbuild {
        Sbuild {
            cache_dir: config
                .build_env
                .sbuild_cache_dir
                .clone()
                .unwrap_or("~/.cache/sbuild".to_string()),
            config,
            build_files_dir,
        }
    }

    fn get_additional_deps(&self) -> Vec<String> {
        let package_type = &self.config.package_type;
        let lang_env = match package_type {
            PackageType::Default(config) => Some(&config.language_env),
            PackageType::Git(config) => Some(&config.language_env),
            PackageType::Virtual => None,
        };
        match lang_env {
            None => {
                vec![]
            }
            Some(lang_env) => {
                let mut additional_deps: Vec<String> = vec![];
                let lang_deps = match lang_env {
                    LanguageEnv::C => {
                        let lang_deps = vec![];
                        lang_deps
                    }
                    LanguageEnv::Rust(config) => {
                        // TODO
                        // let rust_version = &config.rust_version;
                        let rust_binary_url = &config.rust_binary_url;
                        let rust_binary_gpg_asc = &config.rust_binary_gpg_asc;
                        let lang_deps = vec![
                            "apt install -y curl gpg gpg-agent".to_string(),
                            format!("cd /tmp && curl -o rust.tar.xz -L {}", rust_binary_url),
                            format!("cd /tmp && echo \"{}\" >> rust.tar.xz.asc && cat rust.tar.xz.asc ", rust_binary_gpg_asc),
                            "curl https://keybase.io/rust/pgp_keys.asc | gpg --import".to_string(),

                            "cd /tmp && gpg --verify rust.tar.xz.asc rust.tar.xz".to_string(),
                            "cd /tmp && tar xvJf rust.tar.xz -C . --strip-components=1 --exclude=rust-docs".to_string(),
                            "cd /tmp && /bin/bash install.sh --without=rust-docs".to_string(),
                            "apt remove -y curl gpg gpg-agent".to_string()
                        ];
                        lang_deps
                    }
                    LanguageEnv::Go(config) => {
                        // TODO
                        //let go_version = &config.go_version;
                        let go_binary_url = &config.go_binary_url;
                        let go_binary_checksum = &config.go_binary_checksum;
                        let install = vec![
                            "apt install -y curl".to_string(),
                            format!("cd /tmp && curl -o go.tar.gz -L {}", go_binary_url),
                            format!("cd /tmp && echo \"{} go.tar.gz\" >> hash_file.txt && cat hash_file.txt", go_binary_checksum),
                            "cd /tmp && sha256sum -c hash_file.txt".to_string(),
                            "cd /tmp && rm -rf /usr/local/go && mkdir /usr/local/go && tar -C /usr/local -xzf go.tar.gz".to_string(),
                            "ln -s /usr/local/go/bin/go /usr/bin/go".to_string(),
                            "go version".to_string(),
                            "apt remove -y curl".to_string(),
                        ];
                        install
                    }
                    LanguageEnv::JavaScript(config) | LanguageEnv::TypeScript(config) => {
                        // TODO node version
                        // from nodesource
                        // TODO switch from nodesource to actual binary without repository
                        let mut install = vec![
                            "curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - && apt-get install -y nodejs npm".to_string(),
                            "node --version".to_string(),
                            "npm --version".to_string(),
                        ];
                        if let Some(yarn_version) = &config.yarn_version {
                            install.push(format!("npm install --global yarn@{}", yarn_version));
                            install.push("yarn --version".to_string());
                        }
                        install
                    }
                    LanguageEnv::Java(config) => {
                        let is_oracle = config.is_oracle;
                        if is_oracle {
                            let jdk_version = &config.jdk_version;
                            let jdk_binary_url = &config.jdk_binary_url;
                            let jdk_binary_checksum = &config.jdk_binary_checksum;
                            let install = vec![
                                "apt install -y wget".to_string(),
                                format!("mkdir -p /opt/lib/jvm/jdk-{version}-oracle && mkdir -p /usr/lib/jvm", version = jdk_version),
                                format!("cd /tmp && wget -q --output-document jdk.tar.gz {}", jdk_binary_url),
                                format!("cd /tmp && echo \"{} jdk.tar.gz\" >> hash_file.txt && cat hash_file.txt", jdk_binary_checksum),
                                "cd /tmp && sha256sum -c hash_file.txt".to_string(),
                                format!("cd /tmp && tar -zxf jdk.tar.gz -C /opt/lib/jvm/jdk-{version}-oracle --strip-components=1", version = jdk_version),
                                format!("ln -s /opt/lib/jvm/jdk-{version}-oracle/bin/java  /usr/bin/java", version = jdk_version),
                                format!("ln -s /opt/lib/jvm/jdk-{version}-oracle/bin/javac  /usr/bin/javac", version = jdk_version),
                                "java -version".to_string(),
                                "apt remove -y wget".to_string(),
                            ];
                            return install;
                        }
                        vec![]
                    }
                    LanguageEnv::Dotnet(config) => {
                        let dotnet_version = &config.dotnet_version;
                        // TODO do not use MS repository as they upgrade between major versions
                        // this breaks backward compatibility
                        // reproducible builds should use pinned versions
                        let install = vec![
                            "apt install -y wget".to_string(),
                            "cd /tmp && wget https://packages.microsoft.com/config/debian/12/packages-microsoft-prod.deb -O packages-microsoft-prod.deb".to_string(),
                            "cd /tmp && dpkg -i packages-microsoft-prod.deb ".to_string(),
                            "apt-get update -y".to_string(),
                            format!("apt-get install -y dotnet-sdk-{}", dotnet_version),
                            "dotnet --version".to_string(),
                            "apt remove -y wget".to_string(),
                        ];
                        install
                    }
                    LanguageEnv::Nim(config) => {
                        let nim_version = &config.nim_version;
                        let nim_binary_url = &config.nim_binary_url;
                        let nim_version_checksum = &config.nim_version_checksum;
                        let install = vec![
                            "apt install -y wget".to_string(),
                            format!("rm -rf /tmp/nim-{version} && rm -rf /usr/lib/nim/nim-{version}&& rm -rf /opt/lib/nim/nim-{version} && mkdir /tmp/nim-{version}", version = nim_version),
                            "mkdir -p /opt/lib/nim && mkdir -p /usr/lib/nim".to_string(),
                            format!("cd /tmp && wget -q {}", nim_binary_url),
                            format!("cd /tmp && echo {} >> hash_file.txt && cat hash_file.txt", nim_version_checksum),
                            "cd /tmp && sha256sum -c hash_file.txt".to_string(),
                            format!("cd /tmp && tar xJf nim-{version}-linux_x64.tar.xz -C nim-{version} --strip-components=1", version = nim_version),
                            format!("cd /tmp  && mv nim-{version} /opt/lib/nim", version = nim_version),
                            format!("ln -s /opt/lib/nim/nim-{version}/bin/nim  /usr/bin/nim", version = nim_version),
                            // equality check not working
                            //  format!("installed_version=`nim --version | head -n 1 | awk '{{print $4}}'` && echo \"installed version: $installed_version\" && [ \"$installed_version\" != \"{}\" ] && exit 1", nim_version),
                            "nim --version".to_string(),
                            "apt remove -y wget".to_string(),
                        ];
                        install
                    }
                };
                // let is_docker_needed_for_tests = true;
                // if is_docker_needed_for_tests {
                //     // Note this doesn't install docker, please put into that into build_depends
                //     let install = vec![
                //         "apt install -y gnupg".to_string(),
                //         "install -m 0755 -d /etc/apt/keyrings".to_string(),
                //         "curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc".to_string(),
                //         "chmod a+r /etc/apt/keyrings/docker.asc".to_string(),
                //         "echo deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian bookworm stable | \
                //                 tee /etc/apt/sources.list.d/docker.list > /dev/null".to_string(),
                //         "apt-get update".to_string(),
                //         "apt-get remove gnupg".to_string()
                //     ];
                //     additional_deps.extend(install);
                additional_deps.extend(lang_deps);

                // additional_deps.push(format!("apt remove -y {}", additional_build_deps_for_langs));
                additional_deps
            }
        }
    }

    pub fn get_cache_file(&self) -> String {
        let dir = &self.cache_dir;
        let expanded_path = if dir.starts_with('~') {
            let expanded_path = shellexpand::tilde(dir).to_string();
            expanded_path
        } else if dir.starts_with('/') {
            self.cache_dir.clone()
        } else {
            let parent_dir = env::current_dir().unwrap();
            let dir = parent_dir.join(dir);
            let path = fs::canonicalize(dir.clone()).unwrap();
            let path = path.to_str().unwrap().to_string();
            path
        };
        let cache_file_name = format!(
            "{}-{}.tar.gz",
            self.config.build_env.codename, self.config.build_env.arch
        )
        .to_string();
        let path = Path::new(&expanded_path);
        let cache_file = path.join(cache_file_name);
        cache_file.to_str().unwrap().to_string()
    }
}

impl BackendBuildEnv for Sbuild {
    fn clean(&self) -> Result<()> {
        let cache_file = self.get_cache_file();
        info!("Cleaning cached build: {}", cache_file);
        let path = Path::new(&cache_file);
        if path.exists() {
            remove_file_or_directory(&cache_file, false)
                .map_err(|_| eyre!("Could not remove previous cache file!"))?;
        }
        Ok(())
    }

    fn create(&self) -> Result<()> {
        let mut temp_dir = env::temp_dir();
        let dir_name = format!("temp_{}", random::<u32>());
        temp_dir.push(dir_name);
        fs::create_dir(&temp_dir)?;

        let cache_dir = self.get_cache_file();

        if self.config.build_env.codename != "bookworm" {
            return Err(eyre!("Only bookworm supported at the moment!"));
        }
        let create_result = Command::new("sbuild-createchroot")
            .arg("--chroot-mode=unshare")
            .arg("--make-sbuild-tarball")
            .arg(cache_dir)
            .arg(&self.config.build_env.codename)
            .arg(temp_dir)
            .arg("http://deb.debian.org/debian")
            .status();

        if let Err(err) = create_result {
            return Err(eyre!(format!("Failed to create new chroot: {}", err)));
        }
        Ok(())
    }
    fn build(&self) -> Result<()> {
        let mut cmd_args = vec![
            "-d".to_string(),
            self.config.build_env.codename.to_string(),
            "-A".to_string(),                    // build_arch_all
            "-s".to_string(),                    // build source
            "--source-only-changes".to_string(), // source_only_changes
            "-c".to_string(),                    // override cache file location, default is ~/.cache/sbuild both by sbuild and pkg-builder
            self.get_cache_file(),
            "-v".to_string(),                    // verbose
            "--chroot-mode=unshare".to_string(),
        ];

        let lang_deps = self.get_additional_deps();

        for action in lang_deps.iter() {
            cmd_args.push(format!("--chroot-setup-commands={}", action))
        }
        cmd_args.push("--chroot-setup-commands=apt dist-upgrade".to_string());
        cmd_args.push("--chroot-setup-commands=apt autoremove -y && cat".to_string());

        if let Some(true) = self.config.build_env.run_lintian {
        } else {
            cmd_args.push("--no-run-lintian".to_string());
        }
        if let Some(true) = self.config.build_env.run_autopkgtest {
        } else {
            cmd_args.push("--no-run-autopkgtest".to_string());
        }
        if let Some(true) = self.config.build_env.run_piuparts {
        } else {
            cmd_args.push("--no-run-piuparts".to_string());
        }
        println!(
            "Building package by invoking: sbuild {}",
            cmd_args.join(" ")
        );

        let mut child = Command::new("sbuild")
            .current_dir(self.build_files_dir.clone())
            .args(&cmd_args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);

            for line in reader.lines() {
                let line = line?;
                println!("{}", line);
            }
        }
        io::stdout().flush()?;

        child.wait().map_err(|err| eyre!(err.to_string()))?;

        Ok(())
    }
}

fn remove_file_or_directory(path: &str, is_directory: bool) -> io::Result<()> {
    if is_directory {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use env_logger::Env;
    use std::fs::File;
    use std::sync::Once;
    use tempfile::tempdir;

    static INIT: Once = Once::new();

    // Set up logging for tests
    fn setup() {
        INIT.call_once(|| {
            env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
        });
    }

    #[test]
    fn test_clean_sbuild_env_when_file_does_not_exist() {
        setup();
        let mut pkg_config = PkgConfig::default();
        let build_files_dir = tempdir().unwrap().path().to_str().unwrap().to_string();
        pkg_config.build_env.codename = "bookworm".to_string();
        pkg_config.build_env.arch = "amd64".to_string();
        let sbuild_cache_dir = tempdir().unwrap().path().to_str().unwrap().to_string();
        pkg_config.build_env.sbuild_cache_dir = Some(sbuild_cache_dir);
        let build_env = Sbuild::new(pkg_config, build_files_dir);
        let result = build_env.clean();
        assert!(result.is_ok());
        let cache_file = build_env.get_cache_file();
        let cache_file_path = Path::new(&cache_file);
        assert!(!cache_file_path.exists())
    }

    #[test]
    fn test_clean_sbuild_env() {
        setup();
        let mut pkg_config = PkgConfig::default();
        let build_files_dir = tempdir().unwrap().path().to_str().unwrap().to_string();
        pkg_config.build_env.codename = "bookworm".to_string();
        pkg_config.build_env.arch = "amd64".to_string();
        let sbuild_cache = tempdir().unwrap();
        // create dir manually, as it doesn't exist
        fs::create_dir_all(sbuild_cache.path()).expect("Could not create temporary directory for testing.");
        let sbuild_cache_dir = sbuild_cache.path().to_str().unwrap().to_string();
        pkg_config.build_env.sbuild_cache_dir = Some(sbuild_cache_dir.clone());
        let build_env = Sbuild::new(pkg_config, build_files_dir);
        let cache_file = build_env.get_cache_file();
        let cache_file_path = Path::new(&cache_file);

        File::create(cache_file_path)
            .expect("File needs to be created manually before testing deletion.");
        assert!(
            Path::new(&sbuild_cache_dir).exists(),
        );

        assert!(
            cache_file_path.exists(),
            "File should exist before testing deletion."
        );

        let result = build_env.clean();
        assert!(result.is_ok());
        assert!(!cache_file_path.exists())
    }

    #[test]
    fn test_create_sbuild_env() {
        setup();
        let mut pkg_config = PkgConfig::default();
        pkg_config.build_env.codename = "bookworm".to_string();
        pkg_config.build_env.arch = "amd64".to_string();
        let sbuild_cache_dir = tempdir().unwrap().path().to_str().unwrap().to_string();
        pkg_config.build_env.sbuild_cache_dir = Some(sbuild_cache_dir);

        let build_files_dir = tempdir().unwrap().path().to_str().unwrap().to_string();
        let build_env = Sbuild::new(pkg_config, build_files_dir);
        build_env.clean().expect("Could not clean previous env.");
        let cache_file = build_env.get_cache_file();
        let cache_file_path = Path::new(&cache_file);
        assert!(!cache_file_path.exists());
        let result = build_env.create();
        assert!(result.is_ok());
        assert!(cache_file_path.exists())
    }
}