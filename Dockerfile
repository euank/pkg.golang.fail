FROM rust:buster as builder

COPY ./ /app

WORKDIR /app

RUN cargo build --release

FROM rust:buster as runtime

RUN mkdir -p /app /data
COPY --from=builder /app/target/release/pkg-golang-fail /app/pkg-golang-fail

WORKDIR /data

ENTRYPOINT ["/app/pkg-golang-fail"]
