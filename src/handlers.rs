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