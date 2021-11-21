use bollard::{Docker, API_DEFAULT_VERSION};

use manager::ContainerManager;
use warp::Filter;

mod config;
mod filters;
mod handlers;
mod manager;

const CONTAINER_LABEL: &str = "prometheus.makepress.containers";
const DB_LABEL: &str = "prometheus.makepress.db";

#[macro_export]
macro_rules! const_expr_count {
    () => (0);
    ($e:expr) => (1);
    ($e:expr; $($other_e:expr);*) => ({
        1 $(+ $crate::const_expr_count!($other_e) )*
    });
    ($e:expr; $($other_e:expr);* ;) => (
        $crate::const_expr_count! { $e; $(other_e);* }
    );
}

#[macro_export]
macro_rules! hash_map {
    (with $map:expr; insert { $($key:expr => $val:expr),* , }) => {
        $crate::hash_map!(with $map; insert { $($key => $val),*})
    };
    (with $map:expr; insert { $($key:expr => $val:expr),* }) => ({
        let count = const_expr_count!($($key);*);
        #[allow(unused_mut)]
        let mut map = $map;
        map.reserve(count);
        $(
            map.insert($key, $val);
        )*
        map
    });
    ($($key:expr => $val:expr),* ,) => (
        $crate::hash_map!($($key => $val),*)
    );
    ($($key:expr => $val:expr),*) => ({
        let start_capacity = const_expr_count!($($key);*);
        #[allow(unused_mut)]
        let mut map = ::std::collections::HashMap::with_capacity(start_capacity);
        $( map.insert($key, $val); )*
        map
    });
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "makepress=info");
    }
    pretty_env_logger::init();

    let docker = Docker::connect_with_unix("/var/run/docker.sock", 120, API_DEFAULT_VERSION)?;
    let inst: ContainerManager = docker.into();

    inst.init().await?;

    println!("STARTED!");
    warp::serve(filters::all(inst).with(warp::log("makepress")))
        .run(([0, 0, 0, 0], 8080))
        .await;

    Ok(())
}
