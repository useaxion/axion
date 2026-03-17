mod assets;
mod ipc;
mod module;
mod modules;
mod permissions;
mod rpc;

fn main() {
    // Load and validate permissions.json before doing anything else.
    // A missing or malformed file is a hard startup failure — the app cannot
    // run without knowing what capabilities it has declared.
    let permissions_path = std::path::Path::new("permissions.json");
    let _permissions = permissions::Permissions::load(permissions_path).unwrap_or_else(|e| {
        eprintln!("Axion: failed to load permissions.json — {e}");
        std::process::exit(1);
    });

    let mode = assets::RuntimeMode::detect();
    match &mode {
        assets::RuntimeMode::Dev { url } => {
            println!("Axion runtime starting... [dev mode → {url}]");
        }
        assets::RuntimeMode::Production => {
            println!("Axion runtime starting... [production — assets embedded]");
        }
    }
}
