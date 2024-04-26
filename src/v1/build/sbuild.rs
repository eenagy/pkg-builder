use crate::v1::packager::BackendBuildEnv;
use crate::v1::pkg_config::{LanguageEnv, PackageType, PkgConfig};
use eyre::{eyre, Result};
use log::info;
use rand::random;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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

    fn get_build_deps_based_on_langenv(&self, lang_env: &LanguageEnv) -> Vec<String> {
        match lang_env {
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
                    "apt remove -y curl gpg gpg-agent".to_string(),
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
                    // dynamically link to go shared libs, not statically, required in debian
                    "go install -buildmode=shared std".to_string(),
                    // add write permission, this is a chroot env, with one user, should be fine
                    "chmod -R a+rwx /usr/local/go/pkg".to_string(),
                    "apt remove -y curl".to_string(),
                ];
                install
            }
            LanguageEnv::JavaScript(config) | LanguageEnv::TypeScript(config) => {
                // let node_version = &config.go_version;
                let node_binary_url = &config.node_binary_url;
                let node_binary_checksum = &config.node_binary_checksum;
                let mut install = vec![
                    "apt install -y curl".to_string(),
                    format!("cd /tmp && curl -o node.tar.gz -L {}", node_binary_url),
                    format!("cd /tmp && echo \"{} node.tar.gz\" >> hash_file.txt && cat hash_file.txt", node_binary_checksum),
                    "cd /tmp && sha256sum -c hash_file.txt".to_string(),
                    "cd /tmp && rm -rf /usr/share/node && mkdir /usr/share/node && tar -C /usr/share/node -xzf node.tar.gz --strip-components=1".to_string(),
                    "ls -l /usr/share/node/bin".to_string(),
                    "ln -s /usr/share/node/bin/node /usr/bin/node".to_string(),
                    "ln -s /usr/share/node/bin/npm /usr/bin/npm".to_string(),
                    "ln -s /usr/share/node/bin/npx /usr/bin/npx".to_string(),
                    "ln -s /usr/share/node/bin/corepack /usr/bin/corepack".to_string(),
                    "apt remove -y curl".to_string(),
                    "node --version".to_string(),
                    "npm --version".to_string(),
                ];
                if let Some(yarn_version) = &config.yarn_version {
                    install.push(format!("npm install --global yarn@{}", yarn_version));
                    install.push("ln -s /usr/share/node/bin/yarn /usr/bin/yarn".to_string());
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
                    let mut install = vec![
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
                    if let Some(gradle_config) = &config.gradle {
                        let gradle_version = &gradle_config.gradle_version;
                        let gradle_binary_url = &gradle_config.gradle_binary_url;
                        let gradle_binary_checksum = &gradle_config.gradle_binary_checksum;

                        install.push("apt install -y wget unzip".to_string());
                        install.push(format!("mkdir -p /opt/lib/gradle-{version}", version = gradle_version));
                        install.push(format!("cd /tmp && wget -q --output-document gradle.tar.gz {}", gradle_binary_url));
                        install.push(format!("cd /tmp && echo \"{} gradle.tar.gz\" > hash_file.txt && cat hash_file.txt", gradle_binary_checksum));
                        install.push("cd /tmp && sha256sum -c hash_file.txt".to_string());
                        install.push(format!("cd /tmp && unzip gradle.tar.gz && mv gradle-{version} /opt/lib", version = gradle_version));
                        install.push(format!("ln -s /opt/lib/gradle-{version}/bin/gradle  /usr/bin/gradle", version = gradle_version));
                        install.push("gradle -version".to_string());
                        install.push("apt remove -y wget".to_string());
                    }
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
        }
    }
    fn get_build_deps_not_in_debian(&self) -> Vec<String> {
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
                self.get_build_deps_based_on_langenv(lang_env)
            }
        }
    }
    fn get_test_deps_based_on_langenv(&self, lang_env: &LanguageEnv) -> Vec<String> {
        match lang_env {
            LanguageEnv::C => {
                let lang_deps = vec![];
                lang_deps
            }
            LanguageEnv::Rust(_) => {
                // rust compiles to binary, no need to install under test_bed
                let lang_deps = vec![];
                lang_deps
            }
            LanguageEnv::Go(_) => {
                // go compiles to binary, no need to install under test_bed
                let lang_deps = vec![];
                lang_deps
            }
            LanguageEnv::JavaScript(_) | LanguageEnv::TypeScript(_) => {
                // do not install node, as we cannot depend on it, make the testbed install it
                // let node_version = &config.go_version;
                let lang_deps = vec![];
                lang_deps
            }
            LanguageEnv::Java(_) => {
                // do not install jdk, or gradle, as we cannot depend on it, make the testbed install it
                // let node_version = &config.go_version;
                let lang_deps = vec![];
                lang_deps
            }
            LanguageEnv::Dotnet(_) => {
                // add ms repo, but do not install dotnet, let test_bed add it as intall dependency
                let install = vec![
                    "apt install -y wget".to_string(),
                    "cd /tmp && wget https://packages.microsoft.com/config/debian/12/packages-microsoft-prod.deb -O packages-microsoft-prod.deb".to_string(),
                    "cd /tmp && dpkg -i packages-microsoft-prod.deb ".to_string(),
                    "apt-get update -y".to_string(),
                    "apt remove -y wget".to_string(),
                ];
                install
            }
            LanguageEnv::Nim(_) => {
                // nim compiles to binary, no need to install under test_bed
                let lang_deps = vec![];
                lang_deps
            }
        }
    }
    fn get_test_deps_not_in_debian(&self) -> Vec<String> {
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
                self.get_test_deps_based_on_langenv(lang_env)
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

    pub fn get_deb_dir(&self) -> &Path {
        let deb_dir = Path::new(&self.build_files_dir).parent().unwrap();
        deb_dir
    }
    pub fn get_deb_name(&self) -> PathBuf {
        let deb_dir = self.get_deb_dir();
        let deb_file_name = format!("{}_{}-{}_{}.deb",
                                    self.config.package_fields.package_name,
                                    self.config.package_fields.version_number,
                                    self.config.package_fields.revision_number,
                                    self.config.build_env.arch);
        let deb_name = deb_dir.join(deb_file_name);
        deb_name
    }

    //hello-world_1.0.0-1_amd64.changes
    pub fn get_changes_file(&self) -> PathBuf {
        let deb_dir = self.get_deb_dir();
        let deb_file_name = format!("{}_{}-{}_{}.changes",
                                    self.config.package_fields.package_name,
                                    self.config.package_fields.version_number,
                                    self.config.package_fields.revision_number,
                                    self.config.build_env.arch);
        let deb_name = deb_dir.join(deb_file_name);
        deb_name
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

        let cache_file = self.get_cache_file();
        let cache_dir = Path::new(&cache_file).parent().unwrap();
        fs::create_dir_all(cache_dir).map_err(|_| eyre!("Failed to create cache_dir"))?;

        if self.config.build_env.codename != "bookworm" {
            return Err(eyre!("Only bookworm supported at the moment!"));
        }
        let create_result = Command::new("sbuild-createchroot")
            .arg("--chroot-mode=unshare")
            .arg("--make-sbuild-tarball")
            .arg(cache_file)
            .arg(&self.config.build_env.codename)
            .arg(temp_dir)
            .arg("http://deb.debian.org/debian")
            .status();

        if let Err(err) = create_result {
            return Err(eyre!(format!("Failed to create new chroot: {}", err)));
        }
        Ok(())
    }
    fn package(&self) -> Result<()> {
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

        let lang_deps = self.get_build_deps_not_in_debian();

        for action in lang_deps.iter() {
            cmd_args.push(format!("--chroot-setup-commands={}", action))
        }


        if let Some(true) = self.config.build_env.run_lintian {
            cmd_args.push("--run-lintian".to_string());
            cmd_args.push("--lintian-opt=--suppress-tags".to_string());
            cmd_args.push("--lintian-opt=bad-distribution-in-changes-file".to_string());
            cmd_args.push("--lintian-opts=-i".to_string());
            cmd_args.push("--lintian-opts=--I".to_string());
        } else {
            cmd_args.push("--no-run-lintian".to_string());
        }

        println!(
            "Building package by invoking: sbuild {}",
            cmd_args.join(" ")
        );

        let mut cmd = Command::new("sbuild")
            .current_dir(self.build_files_dir.clone())
            .args(&cmd_args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        run_process(&mut cmd)?;

        if let Some(true) = self.config.build_env.run_piuparts {
            self.run_piuparts()?;
        };
        if let Some(true) = self.config.build_env.run_autopkgtest {
            self.run_autopkgtests()?;
        }
        Ok(())
    }

    fn run_piuparts(&self) -> Result<()> {
        println!(
            "Running piuparts command with elevated privileges..",
        );
        println!(
            "Piuparts must run as root user through sudo, please provide your password, if prompted."
        );

        let mut cmd_args = vec![
            "-d".to_string(),
            self.config.build_env.codename.to_string(),
            "-m".to_string(),
            "http://deb.debian.org/debian".to_string(),
            "--bindmount=/dev".to_string(),
            // TODO as parameter
            "--keyring=/usr/share/keyrings/debian-archive-keyring.gpg".to_string(),
        ];
        let package_type = &self.config.package_type;

        let lang_env = match package_type {
            PackageType::Default(config) => Some(&config.language_env),
            PackageType::Git(config) => Some(&config.language_env),
            PackageType::Virtual => None,
        };
        if let Some(env) = lang_env {
            match env {
                LanguageEnv::Dotnet(_) => {
                    // let signed_by = "/usr/share/keyrings/microsoft-prod.gpg";
                    // doesn't work as ca-certificates must be installed under chroot before adding repo
                    // chicken and egg problem
                    let ms_repo = format!("deb https://packages.microsoft.com/debian/12/prod {} main", self.config.build_env.codename);
                    cmd_args.push(format!("--extra-repo={}", ms_repo));
                    cmd_args.push("--do-not-verify-signatures".to_string());
                }
                _ => {
                    // no other package repositories supported
                    // might supply my own, but not for now
                }
            }
        }
        let deb_dir = self.get_deb_dir();
        let deb_name = self.get_deb_name();
        println!(
            "Testing package by invoking: piuparts with: sudo -S piuparts {} {}",
            cmd_args.join(" "),
            deb_name.to_str().unwrap()
        );
        println!("Note this command run inside of directory: {}", deb_dir.display());

        let mut cmd = Command::new("sudo")
            .current_dir(deb_dir)
            // for CI
            .arg("-S")
            .arg("piuparts")
            .args(&cmd_args)
            .arg(deb_name)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        run_process(&mut cmd)
    }

    fn run_autopkgtests(&self) -> Result<()> {
        println!(
            "Running autopkgtests command",
        );

        let image_name =  format!("autopkgtest-{}.img",  self.config.build_env.codename.to_string());
        let mut cache_dir = self.cache_dir.clone();
        if cache_dir.starts_with('~'){
            cache_dir = shellexpand::tilde(&cache_dir).to_string()
        }
        let image_path = Path::new(&cache_dir).join(image_name.clone());
        create_autopkgtest_image(image_path.clone(), self.config.build_env.codename.to_string())?;

        let deb_dir = self.get_deb_dir();
      //  let deb_name = self.get_deb_name();
        let changes_file = self.get_changes_file();
        let mut cmd_args = vec![
            changes_file.to_str().unwrap().to_string(),
            // this will not going rebuild the package, which we want to avoid
            // as some packages can take an hour to build,
            // we don't want to build for 2 hours
            "--no-built-binaries".to_string(),
        ];
        let lang_deps = self.get_test_deps_not_in_debian();

        for action in lang_deps.iter() {
            cmd_args.push(format!("--chroot-setup-commands={}", action))
        }
        cmd_args.push("--".to_string());
        cmd_args.push("qemu".to_string());
        cmd_args.push(image_path.to_str().unwrap().to_string());
        println!(
            "Testing package by invoking: autopkgtest {}",
            cmd_args.join(" ")
        );
        println!("Note this command run inside of directory: {}", deb_dir.display());
        let mut cmd = Command::new("autopkgtest")
            .current_dir(deb_dir)
            .args(&cmd_args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        run_process(&mut cmd)
    }
}

fn create_autopkgtest_image(image_path: PathBuf, codename: String) -> Result<()>{

    // do not recreate image if exists
    if image_path.exists() {
        return Ok(())
    }
    println!(
        "autopkgtests environment does not exist. Creating it."
    );
    println!(
        "please provide your password through sudo to as autopkgtest env creation requires it."
    );
    let cmd_args = vec![
        codename.clone(),
        image_path.to_str().unwrap().to_string(),
    ];
    let mut cmd = Command::new("sudo")
        // for CI
        .arg("-S")
        .arg("autopkgtest-build-qemu")
        .args(&cmd_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    run_process(&mut cmd)
}
fn run_process(child: &mut Child) -> Result<()> {
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            let line = line?;
            println!("{}", line);
        }
    }
    io::stdout().flush()?;

    let status = child.wait().map_err(|err| eyre!(err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(eyre!("Sbuild exited with non-zero status code. Please see build output for potential causes."))
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
