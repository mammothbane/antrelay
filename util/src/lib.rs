#![feature(const_trait_impl)]
#![feature(const_option_ext)]

mod build;
mod futures;
mod macros;

#[tracing::instrument(skip_all, level = "trace")]
pub fn brotli_compress(message: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let compressed_bytes = {
        let mut out = vec![];

        lazy_static::lazy_static! {
            static ref PARAMS: brotli::enc::BrotliEncoderParams = brotli::enc::BrotliEncoderParams {
                quality: 11,
                ..Default::default()
            };
        }

        brotli::BrotliCompress(&mut &message[..], &mut out, &*PARAMS)?;

        out
    };

    Ok(compressed_bytes)
}

#[tracing::instrument(skip_all, level = "trace")]
pub fn brotli_decompress(v: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut out = vec![];
    brotli::BrotliDecompress(&mut &v[..], &mut out)?;

    Ok(out)
}
