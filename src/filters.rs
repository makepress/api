use warp::Filter;

use crate::ContainerManager;

pub fn all(
    manager: ContainerManager,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    list_containers(manager.clone()).or(create_container(manager))
}

fn with_manager(
    manager: ContainerManager,
) -> impl Filter<Extract = (ContainerManager,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || manager.clone())
}

/// GET /list
fn list_containers(
    manager: ContainerManager,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("list")
        .and(warp::get())
        .and(with_manager(manager))
        .and_then(crate::handlers::list_containers)
}

/// POST /create/:name with JSON body
fn create_container(
    manager: ContainerManager,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("create" / String)
        .and(warp::post())
        .and(with_manager(manager))
        .and_then(crate::handlers::create_container)
}
