FROM rustembedded/cross:x86_64-unknown-linux-gnu AS x86_64-builder

ARG UID=1000
ARG GID=1000

ENV TOOLCHAIN=stable
ENV TARGET=x86_64-unknown-linux-gnu
ENV TOOL=x86_64-linux-gnu

RUN \
 apt-get update && \
 apt-get upgrade -y

RUN \
 addgroup --gid "${GID}" build && \
 adduser \
    --disabled-password \
    --gecos "" \
    --ingroup build \
    --uid "${UID}" \
    --home /opt/build \
    build

ADD https://sh.rustup.rs /opt/build/rustup.sh

RUN \
 chown -R build:build /opt/build

USER build
WORKDIR /opt/build

ENV PATH=$PATH:/opt/build/.cargo/bin

RUN \
 chmod +x rustup.sh && \
 ./rustup.sh --default-toolchain $TOOLCHAIN --profile minimal -y && \
 rustup target add $TARGET

FROM x86_64-builder as builder

USER root

RUN \
 dpkg --add-architecture amd64 && \
 apt-get update && \
 apt-get -y install libgexiv2-dev:amd64

USER build
ENV USER=build

# Cache deps
RUN \
 cargo new repo

WORKDIR /opt/build/repo

COPY Cargo.toml Cargo.lock ./

USER root
RUN \
 chown -R build:build ./ 

USER build

RUN \
 mkdir -p ./src && \
 echo 'fn main() { println!("Dummy") }' > ./src/main.rs && \
 cargo build --release && \
 rm -rf ./src

COPY src ./src/

USER root
RUN \
 chown -R build:build ./src && \
 rm -r ./target/release/deps/pict_rs-*

USER build

# Build for release
RUN cargo build --frozen --release

FROM ubuntu:20.04

ARG UID=1000
ARG GID=1000

RUN apt-get update \
  && apt-get install -y libgexiv2-2

# Copy resources
COPY --from=builder /opt/build/repo/target/release/pict-rs /usr/bin/pict-rs

RUN \
 addgroup -gid "${GID}" pictrs && \
 adduser \
    --disabled-password \
    --gecos "" \
    --ingroup pictrs \
    --uid "${UID}" \
    --home /opt/pictrs \
    pictrs
WORKDIR /opt/pictrs
USER pictrs
EXPOSE 8080
CMD ["/usr/bin/pict-rs"]
