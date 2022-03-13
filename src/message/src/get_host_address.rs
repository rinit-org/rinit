use nix::unistd::Uid;

lazy_static! {
    static ref UID: Uid = Uid::current();
    static ref HOST: String = if UID.is_root() {
        "/run/kansei/.socket".to_string()
    } else {
        format!("/run/user/{}/kansei/.socket", UID.as_raw())
    };
}

pub fn get_host_address() -> &'static str {
    &HOST
}
