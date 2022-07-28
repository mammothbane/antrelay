FROM lukemathwalker/cargo-chef@sha256:67477ba57116777bca4f515d18d3dc275b5060b408c5a32f300d98dcf9337814 AS chef

WORKDIR /src
COPY rust-toolchain.toml .
RUN rustup show

FROM chef as prep

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder

COPY --from=prep /src/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json

COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl --bin antrelay

FROM gcr.io/distroless/static@sha256:21d3f84a4f37c36199fd07ad5544dcafecc17776e3f3628baf9a57c8c0181b3f

COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/antrelay /antrelay

ENTRYPOINT ["/antrelay"]

CMD [ \
    "--serial_port", \
    "/serial", \
    "--baud", \
    "115200", \
    \
    "--uplink", \
    "/sockets/uplink", \
    \
    "--downlink", \
    "/sockets/downlink" \
]

