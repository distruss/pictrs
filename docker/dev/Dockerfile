# Build 
FROM ekidd/rust-musl-builder:1.42.0-openssl11 as rust

# Cache deps
WORKDIR /app
RUN sudo chown -R rust:rust .
RUN USER=root cargo new server
WORKDIR /app/server
COPY Cargo.toml Cargo.lock ./
RUN sudo chown -R rust:rust .
RUN mkdir -p ./src/bin \
  && echo 'fn main() { println!("Dummy") }' > ./src/bin/main.rs 
RUN cargo build --release
RUN rm -f ./target/x86_64-unknown-linux-musl/release/deps/pict_rs*
COPY src ./src/

# Build for release
RUN cargo build --frozen --release

FROM alpine:3.10

# Copy resources
COPY --from=rust /app/server/target/x86_64-unknown-linux-musl/release/pict-rs /app/pict-rs

RUN addgroup -g 1000 pictrs
RUN adduser -D -s /bin/sh -u 1000 -G pictrs pictrs
RUN chown pictrs:pictrs /app/pict-rs
USER pictrs
EXPOSE 8080
CMD ["/app/pict-rs"]
