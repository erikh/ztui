use std::path::PathBuf;

pub fn config_path() -> PathBuf {
    directories::UserDirs::new()
        .expect("could not locate your home directory")
        .home_dir()
        .join(".networks.zerotier")
}
