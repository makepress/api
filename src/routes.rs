use warp::{fs::File, Filter, Rejection};

pub(crate) fn all() -> impl Filter<Extract = (File,), Error = Rejection> + Clone {
    warp::path!("backups" / "download")
        .and(warp::get())
        .and(warp::filters::fs::dir("/backups"))
}
