use warp::Rejection;

use super::ContainerManager;

#[derive(Debug)]
struct Error(bollard::errors::Error);

impl warp::reject::Reject for Error {}

pub async fn list_containers(manager: ContainerManager) -> Result<impl warp::Reply, Rejection> {
    match manager.list_containers().await {
        Ok(containers) => Ok(warp::reply::json(&containers)),
        Err(e) => Err(warp::reject::custom(Error(e))),
    }
}

pub async fn create_container(
    name: String,
    manager: ContainerManager,
) -> Result<impl warp::Reply, Rejection> {
    match manager.create_new_container(name).await {
        Ok(r) => Ok(warp::reply::json(&r)),
        Err(e) => Err(warp::reject::custom(Error(e))),
    }
}
