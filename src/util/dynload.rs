#![allow(unsafe_code)]

use std::{
    os::unix::ffi::OsStrExt as _,
    path::Path,
};

use smol::stream::StreamExt;
use tap::Pipe;

#[tracing::instrument(fields(path = %dir.as_ref().display()), skip(dir))]
pub async fn apply_patches(dir: impl AsRef<Path>) {
    let dir = match smol::fs::read_dir(dir).await {
        Err(e) => {
            tracing::warn!(error = %e, "unable to read lib directory");
            return;
        },
        Ok(x) => x,
    };

    let paths = {
        let mut paths = dir
            .pipe(crate::stream_unwrap!("reading dir entry"))
            .map(|ent| ent.file_name())
            .filter(|name| name.as_bytes().ends_with(b".so"))
            .collect::<Vec<_>>()
            .await;

        paths.sort();

        paths
    };

    paths.into_iter().for_each(|path| {
        let _span = tracing::info_span!("loading dynamic library", path = ?path).entered();

        match unsafe { libloading::Library::new(path) } {
            Ok(_) => tracing::info!("loaded ok"),
            Err(e) => tracing::error!(error = %e, "failed loading"),
        }
    });
}
