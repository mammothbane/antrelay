#![feature(const_trait_impl)]
#![feature(const_option_ext)]
#![feature(const_slice_index)]

pub mod build;
pub mod futures;
mod macros;

#[tracing::instrument(skip_all, level = "trace")]
pub fn brotli_compress(message: &impl AsRef<[u8]>) -> std::io::Result<Vec<u8>> {
    let compressed_bytes = {
        let mut out = vec![];

        lazy_static::lazy_static! {
            static ref PARAMS: brotli::enc::BrotliEncoderParams = brotli::enc::BrotliEncoderParams {
                quality: 11,
                ..Default::default()
            };
        }

        brotli::BrotliCompress(&mut message.as_ref(), &mut out, &*PARAMS)?;

        out
    };

    Ok(compressed_bytes)
}

#[tracing::instrument(skip_all, level = "trace")]
pub fn brotli_decompress(v: &impl AsRef<[u8]>) -> std::io::Result<Vec<u8>> {
    let mut out = vec![];
    brotli::BrotliDecompress(&mut v.as_ref(), &mut out)?;

    Ok(out)
}
