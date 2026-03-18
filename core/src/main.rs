mod assets;
mod ipc;
mod module;
mod modules;
mod permissions;
mod rpc;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // --smoke-test: exercise the RPC stack and exit — used by the CI pipeline
    // to confirm the binary is functional without a live WebView2 window.
    if args.contains(&"--smoke-test".to_string()) {
        run_smoke_test();
        return;
    }

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

/// Run a minimal RPC smoke test and exit.
///
/// Dispatches `system.platform` through the real Rust dispatcher and asserts
/// the response is `{ "platform": "windows" }`. Prints the JSON result on
/// success and exits 0; exits 1 on any failure.
///
/// This path is invoked by the CI smoke-test job after `axion build` produces
/// the release binary:
///
/// ```
/// ./dist/SmokeTestApp.exe --smoke-test
/// ```
fn run_smoke_test() {
    use modules::system::SystemModule;
    use module::AxionModule;
    use rpc::dispatcher::Dispatcher;
    use rpc::schema::{RpcRequest, RpcResponse};

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let response = rt.block_on(async {
        let mut dispatcher = Dispatcher::new();
        SystemModule.register_handlers(&mut dispatcher);

        dispatcher
            .dispatch(RpcRequest {
                id: 1,
                method: "system.platform".into(),
                params: serde_json::json!({}),
            })
            .await
    });

    match response {
        RpcResponse::Success { result, .. } => {
            let platform = result["platform"].as_str().unwrap_or("");
            println!("{}", serde_json::json!({ "platform": platform }));
            if platform == "windows" {
                std::process::exit(0);
            } else {
                eprintln!("smoke-test FAIL: expected platform=windows, got {platform:?}");
                std::process::exit(1);
            }
        }
        RpcResponse::Error { error, .. } => {
            eprintln!("smoke-test FAIL: RPC error {}: {}", error.code, error.message);
            std::process::exit(1);
        }
    }
}
