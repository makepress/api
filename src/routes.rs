use warp::{Filter, Rejection, fs::File};

pub(crate) fn all() -> impl Filter<Extract = (File,), Error = Rejection> + Clone {
    warp::path!("backups" / "download")
    .and(warp::get())
    .and(warp::filters::fs::dir("/backups"))
}