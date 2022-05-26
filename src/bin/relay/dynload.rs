use std::path::Path;

use lunarrelay::util;

#[tracing::instrument(fields(path = %dir.as_ref().display()), skip(dir))]
pub async fn apply_patches(dir: impl AsRef<Path>) {
    use smol::stream::StreamExt;
    use std::os::unix::ffi::OsStrExt as _;

    util::bootstrap!("loading libraries from {}", dir.as_ref().display());
    let dir = match smol::fs::read_dir(dir).await {
        Err(e) => {
            util::bootstrap!("unable to read directory: {}", e);
            return;
        },
        Ok(x) => x,
    };

    let paths = {
        let mut paths = dir
            .filter_map(|ent| {
                if let Err(ref e) = ent {
                    util::bootstrap!("reading dir entry: {}", e);
                }

                ent.ok()
            })
            .map(|ent| ent.file_name())
            .filter(|name| name.as_bytes().ends_with(b".so"))
            .collect::<Vec<_>>()
            .await;

        paths.sort();

        paths
    };

    paths.into_iter().for_each(|path| {
        util::bootstrap!("loading dynamic library: {:?}", path);

        match unsafe { libloading::Library::new(path) } {
            Ok(_) => util::bootstrap!("loaded ok"),
            Err(e) => util::bootstrap!("failed loading: {}", e),
        }
    });
}
